//! Parquet I/O for the Attestrum manifest.
//!
//! PROTECTED schema + writer config per Sprint 3 E3 (founder-approved
//! 2026-05-24 post-cross-check). The on-disk schema, the enum code
//! mappings, and every `WriterProperties` setting in [`writer_properties`]
//! are corpus-incompatible contracts: changing any of them invalidates every
//! previously-emitted Attestrum manifest. Bump [`SCHEMA_VERSION`] + the
//! `attestrum.writer.profile` KeyValue metadata + the protected-system commit
//! footer when changing.
//!
//! See `docs/diagrams/sprint-3/manifest-schema.md` for the canonical layout
//! and `Attestrum Sprint 3 E3 — Parquet schema design recommendation` cross-check
//! responses at `~/Downloads/attestrum-e3/` for the rationale behind every
//! choice (PARQUET_1_0, dict OFF global, stats OFF global, raw Int8 enums,
//! raw Int64 timestamps, `created_by` pinned).

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, BooleanBuilder, FixedSizeBinaryArray, FixedSizeBinaryBuilder,
    Int64Array, Int64Builder, Int8Array, Int8Builder, ListArray, ListBuilder, StringArray,
    StringBuilder, StructArray, UInt32Array, UInt32Builder, UInt64Array, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use attestrum_core::{AttestrumError, Modality, Result, SourceType};
use parquet::arrow::arrow_reader::{ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::{EnabledStatistics, WriterProperties, WriterVersion};
use parquet::format::KeyValue;

use crate::{ManifestEntry, ManifestSignals};

// ============================================================================
// PROTECTED constants
// ============================================================================

/// The on-disk manifest schema version. Pinned in file-level KeyValue metadata
/// as `attestrum.manifest.schema_version`. Bump alongside the writer profile and
/// the PROTECTED commit footer whenever the schema changes.
pub const SCHEMA_VERSION: &str = "2";

/// A short identifier for the EXACT writer-config profile this crate emits.
/// Pinned in file-level KeyValue metadata as `attestrum.writer.profile`. Lets a
/// verifier reject manifests written by an incompatible Attestrum.
pub const WRITER_PROFILE: &str = "parquet-rs-55-zstd3-plain-v1";

/// The `created_by` string Parquet writes into the file footer. PINNED to
/// avoid the parquet-rs default which embeds the parquet-rs crate version
/// (would make patch-version dep bumps byte-breaking).
pub const CREATED_BY: &str = "attestrum-manifest/0.1.0";

/// Parquet KeyValue metadata key for the manifest schema version.
const KV_SCHEMA_VERSION: &str = "attestrum.manifest.schema_version";

/// Parquet KeyValue metadata key for the writer profile identifier.
const KV_WRITER_PROFILE: &str = "attestrum.writer.profile";

// ============================================================================
// Enum code mappings — PROTECTED
// ============================================================================

// Modality codes. Order frozen at v1. Adding a variant requires a SCHEMA_VERSION
// bump + migration. Reordering invalidates every prior manifest.
const MODALITY_TEXT: i8 = 0;
const MODALITY_IMAGE: i8 = 1;
const MODALITY_AUDIO: i8 = 2;
const MODALITY_VIDEO: i8 = 3;
const MODALITY_PDF: i8 = 4;
const MODALITY_OTHER: i8 = 5;

fn modality_to_code(m: Modality) -> i8 {
    match m {
        Modality::Text => MODALITY_TEXT,
        Modality::Image => MODALITY_IMAGE,
        Modality::Audio => MODALITY_AUDIO,
        Modality::Video => MODALITY_VIDEO,
        Modality::Pdf => MODALITY_PDF,
        Modality::Other => MODALITY_OTHER,
    }
}

fn modality_from_code(c: i8) -> Result<Modality> {
    match c {
        MODALITY_TEXT => Ok(Modality::Text),
        MODALITY_IMAGE => Ok(Modality::Image),
        MODALITY_AUDIO => Ok(Modality::Audio),
        MODALITY_VIDEO => Ok(Modality::Video),
        MODALITY_PDF => Ok(Modality::Pdf),
        MODALITY_OTHER => Ok(Modality::Other),
        other => Err(AttestrumError::Internal(format!(
            "manifest: unknown modality code {other}"
        ))),
    }
}

// SourceType codes. Order frozen at v1; same migration story as Modality.
const SOURCE_TYPE_CRAWL: i8 = 0;
const SOURCE_TYPE_PUBLIC_DATASET: i8 = 1;
const SOURCE_TYPE_PRIVATE_LICENSED: i8 = 2;
const SOURCE_TYPE_USER: i8 = 3;
const SOURCE_TYPE_SYNTHETIC: i8 = 4;
const SOURCE_TYPE_OTHER: i8 = 5;

fn source_type_to_code(s: SourceType) -> i8 {
    match s {
        SourceType::Crawl => SOURCE_TYPE_CRAWL,
        SourceType::PublicDataset => SOURCE_TYPE_PUBLIC_DATASET,
        SourceType::PrivateLicensed => SOURCE_TYPE_PRIVATE_LICENSED,
        SourceType::User => SOURCE_TYPE_USER,
        SourceType::Synthetic => SOURCE_TYPE_SYNTHETIC,
        SourceType::Other => SOURCE_TYPE_OTHER,
    }
}

fn source_type_from_code(c: i8) -> Result<SourceType> {
    match c {
        SOURCE_TYPE_CRAWL => Ok(SourceType::Crawl),
        SOURCE_TYPE_PUBLIC_DATASET => Ok(SourceType::PublicDataset),
        SOURCE_TYPE_PRIVATE_LICENSED => Ok(SourceType::PrivateLicensed),
        SOURCE_TYPE_USER => Ok(SourceType::User),
        SOURCE_TYPE_SYNTHETIC => Ok(SourceType::Synthetic),
        SOURCE_TYPE_OTHER => Ok(SourceType::Other),
        other => Err(AttestrumError::Internal(format!(
            "manifest: unknown source_type code {other}"
        ))),
    }
}

// ============================================================================
// Arrow schema
// ============================================================================

/// The PROTECTED Arrow schema for the Attestrum manifest. 16 columns from
/// BUILD-PLAN §4.2 plus the two Sprint 3 binding columns
/// (`input_ordinal`, `occurrence_index`).
///
/// Enum fields (`modality`, `source_type`) ship as raw `Int8` per the E3
/// cross-check (avoids the parquet-rs adaptive dictionary-fallback heuristic).
/// `fetched_at` ships as raw `Int64` (epoch ms) instead of Arrow's
/// `Timestamp(Millisecond, _)` to avoid the timezone-metadata string leak.
/// The `signals` STRUCT is non-null with internal nullable string fields
/// for free-form parser outputs.
pub fn arrow_schema() -> SchemaRef {
    let signals_fields = Fields::from(vec![
        Field::new("robots_disallow", DataType::Boolean, false),
        Field::new("robots_user_agent", DataType::Utf8, true),
        Field::new("ai_txt_disallow", DataType::Boolean, false),
        Field::new("tdmrep_reservation", DataType::Int8, false),
        Field::new("tdmrep_policy_url", DataType::Utf8, true),
        Field::new("aipref_usage_pref", DataType::Utf8, true),
        Field::new("iptc_plus_dmi", DataType::Utf8, true),
        Field::new("c2pa_training_mining", DataType::Utf8, true),
        Field::new("rsl_permits", DataType::Utf8, true),
        Field::new("liccium_tdmai_iscc", DataType::Utf8, true),
        Field::new("liccium_tdmai_allow", DataType::Boolean, true),
        Field::new("cloudflare_ai_train", DataType::Utf8, true),
    ]);
    // Inner item is nullable per Arrow ListBuilder default. Our writer never
    // emits null inner items (an empty chunk_refs is `None` at the outer
    // list level, not an inner null), but the schema must declare nullable
    // to match what ListBuilder produces.
    let chunk_ref_inner = Arc::new(Field::new("item", DataType::FixedSizeBinary(32), true));
    let fields = vec![
        Field::new("document_id", DataType::FixedSizeBinary(32), false),
        Field::new("sha256", DataType::FixedSizeBinary(32), false),
        Field::new("size_bytes", DataType::UInt64, false),
        Field::new("modality", DataType::Int8, false),
        Field::new("mime_type", DataType::Utf8, true),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("source_type", DataType::Int8, true),
        Field::new("source_dataset_id", DataType::Utf8, true),
        Field::new("registered_domain", DataType::Utf8, true),
        Field::new("license_spdx", DataType::Utf8, true),
        Field::new("language", DataType::Utf8, true),
        Field::new("fetched_at", DataType::Int64, true),
        Field::new("signals", DataType::Struct(signals_fields), false),
        Field::new("included", DataType::Boolean, false),
        Field::new("exclusion_reason", DataType::Utf8, true),
        Field::new("chunk_refs", DataType::List(chunk_ref_inner), true),
        Field::new("input_ordinal", DataType::UInt64, false),
        Field::new("occurrence_index", DataType::UInt32, false),
    ];
    Arc::new(Schema::new(fields))
}

// ============================================================================
// Writer config — PROTECTED
// ============================================================================

/// The PROTECTED Parquet writer config for Attestrum manifests. Every setting
/// here is part of the consensus-critical artifact; changing one breaks
/// byte-identity across previously-issued manifests.
///
/// Choices (per E3 cross-check, 2026-05-24):
/// - PARQUET_1_0: simpler PLAIN+RLE encodings, smaller surface area than 2.0.
/// - ZSTD level 3: bit-deterministic for fixed (input, level).
/// - Dictionary encoding disabled globally: removes the adaptive
///   dictionary-fallback heuristic.
/// - Statistics disabled globally: removes stats-truncation drift risk.
/// - Bloom filters disabled: hash seeds and bit ordering have impl drift.
/// - Fixed row count + page size: pins page boundaries.
/// - `created_by` pinned: parquet-rs default embeds the crate version.
/// - KeyValue metadata: schema_version + writer_profile, in deterministic
///   order (vec, not HashMap).
pub fn writer_properties() -> WriterProperties {
    WriterProperties::builder()
        .set_writer_version(WriterVersion::PARQUET_1_0)
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_dictionary_enabled(false)
        .set_statistics_enabled(EnabledStatistics::None)
        .set_bloom_filter_enabled(false)
        .set_max_row_group_size(1_000_000)
        .set_data_page_size_limit(1 << 20)
        .set_data_page_row_count_limit(20_000)
        .set_created_by(CREATED_BY.to_string())
        .set_key_value_metadata(Some(vec![
            KeyValue {
                key: KV_SCHEMA_VERSION.to_string(),
                value: Some(SCHEMA_VERSION.to_string()),
            },
            KeyValue {
                key: KV_WRITER_PROFILE.to_string(),
                value: Some(WRITER_PROFILE.to_string()),
            },
        ]))
        .build()
}

// ============================================================================
// Write path
// ============================================================================

/// Write `entries` to a Parquet manifest at `path`. The caller is
/// responsible for the canonical pipeline ordering:
///
/// ```text
/// crate::assign_input_ordinals(&mut entries);
/// crate::assign_occurrence_indices(&mut entries);
/// // ... parallel hashing phase preserves both ordinals ...
/// crate::sort_entries(&mut entries);
/// io::write_manifest(path, &entries)?;
/// ```
///
/// This call is **single-threaded**. Parallelism in the build pipeline
/// happens upstream (in `attestrum-pipeline`); the Parquet write itself is
/// always one writer. Atomic-rename of the output file is the caller's
/// responsibility — `write_manifest` writes directly to `path`.
pub fn write_manifest(path: &Path, entries: &[ManifestEntry]) -> Result<()> {
    let batch = entries_to_record_batch(entries)?;
    let file = File::create(path).map_err(AttestrumError::Io)?;
    let mut writer = ArrowWriter::try_new(file, arrow_schema(), Some(writer_properties()))
        .map_err(|e| AttestrumError::Internal(format!("parquet writer init: {e}")))?;
    writer
        .write(&batch)
        .map_err(|e| AttestrumError::Internal(format!("parquet write batch: {e}")))?;
    writer
        .close()
        .map_err(|e| AttestrumError::Internal(format!("parquet close: {e}")))?;
    Ok(())
}

fn entries_to_record_batch(entries: &[ManifestEntry]) -> Result<RecordBatch> {
    let n = entries.len();

    let mut document_id = FixedSizeBinaryBuilder::with_capacity(n, 32);
    let mut sha256 = FixedSizeBinaryBuilder::with_capacity(n, 32);
    let mut size_bytes = UInt64Builder::with_capacity(n);
    let mut modality = Int8Builder::with_capacity(n);
    let mut mime_type = StringBuilder::new();
    let mut source_url = StringBuilder::new();
    let mut source_type = Int8Builder::with_capacity(n);
    let mut source_dataset_id = StringBuilder::new();
    let mut registered_domain = StringBuilder::new();
    let mut license_spdx = StringBuilder::new();
    let mut language = StringBuilder::new();
    let mut fetched_at = Int64Builder::with_capacity(n);
    // signals STRUCT — build child arrays then assemble StructArray at the end
    let mut sig_robots_disallow = BooleanBuilder::with_capacity(n);
    let mut sig_robots_user_agent = StringBuilder::new();
    let mut sig_ai_txt_disallow = BooleanBuilder::with_capacity(n);
    let mut sig_tdmrep_reservation = Int8Builder::with_capacity(n);
    let mut sig_tdmrep_policy_url = StringBuilder::new();
    let mut sig_aipref_usage_pref = StringBuilder::new();
    let mut sig_iptc_plus_dmi = StringBuilder::new();
    let mut sig_c2pa_training_mining = StringBuilder::new();
    let mut sig_rsl_permits = StringBuilder::new();
    let mut sig_liccium_tdmai_iscc = StringBuilder::new();
    let mut sig_liccium_tdmai_allow = BooleanBuilder::with_capacity(n);
    let mut sig_cloudflare_ai_train = StringBuilder::new();
    let mut included = BooleanBuilder::with_capacity(n);
    let mut exclusion_reason = StringBuilder::new();
    let mut chunk_refs = ListBuilder::new(FixedSizeBinaryBuilder::new(32));
    let mut input_ordinal = UInt64Builder::with_capacity(n);
    let mut occurrence_index = UInt32Builder::with_capacity(n);

    for entry in entries {
        document_id
            .append_value(entry.document_id)
            .map_err(|e| AttestrumError::Internal(format!("append document_id: {e}")))?;
        sha256
            .append_value(entry.sha256)
            .map_err(|e| AttestrumError::Internal(format!("append sha256: {e}")))?;
        size_bytes.append_value(entry.size_bytes);
        modality.append_value(modality_to_code(entry.modality));
        append_opt_str(&mut mime_type, entry.mime_type.as_deref());
        append_opt_str(&mut source_url, entry.source_url.as_deref());
        match entry.source_type {
            Some(st) => source_type.append_value(source_type_to_code(st)),
            None => source_type.append_null(),
        }
        append_opt_str(&mut source_dataset_id, entry.source_dataset_id.as_deref());
        append_opt_str(&mut registered_domain, entry.registered_domain.as_deref());
        append_opt_str(&mut license_spdx, entry.license_spdx.as_deref());
        append_opt_str(&mut language, entry.language.as_deref());
        match entry.fetched_at {
            Some(t) => fetched_at.append_value(t),
            None => fetched_at.append_null(),
        }
        // signals STRUCT — every row is present (non-null container)
        let s = &entry.signals;
        sig_robots_disallow.append_value(s.robots_disallow);
        append_opt_str(&mut sig_robots_user_agent, s.robots_user_agent.as_deref());
        sig_ai_txt_disallow.append_value(s.ai_txt_disallow);
        sig_tdmrep_reservation.append_value(s.tdmrep_reservation);
        append_opt_str(&mut sig_tdmrep_policy_url, s.tdmrep_policy_url.as_deref());
        append_opt_str(&mut sig_aipref_usage_pref, s.aipref_usage_pref.as_deref());
        append_opt_str(&mut sig_iptc_plus_dmi, s.iptc_plus_dmi.as_deref());
        append_opt_str(
            &mut sig_c2pa_training_mining,
            s.c2pa_training_mining.as_deref(),
        );
        append_opt_str(&mut sig_rsl_permits, s.rsl_permits.as_deref());
        append_opt_str(&mut sig_liccium_tdmai_iscc, s.liccium_tdmai_iscc.as_deref());
        match s.liccium_tdmai_allow {
            Some(v) => sig_liccium_tdmai_allow.append_value(v),
            None => sig_liccium_tdmai_allow.append_null(),
        }
        append_opt_str(
            &mut sig_cloudflare_ai_train,
            s.cloudflare_ai_train.as_deref(),
        );
        included.append_value(entry.included);
        append_opt_str(&mut exclusion_reason, entry.exclusion_reason.as_deref());
        match &entry.chunk_refs {
            Some(refs) => {
                for r in refs {
                    chunk_refs
                        .values()
                        .append_value(r)
                        .map_err(|e| AttestrumError::Internal(format!("append chunk_ref: {e}")))?;
                }
                chunk_refs.append(true);
            }
            None => chunk_refs.append(false),
        }
        input_ordinal.append_value(entry.input_ordinal);
        occurrence_index.append_value(entry.occurrence_index);
    }

    let signals_struct = StructArray::from(vec![
        (
            Arc::new(Field::new("robots_disallow", DataType::Boolean, false)),
            Arc::new(sig_robots_disallow.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("robots_user_agent", DataType::Utf8, true)),
            Arc::new(sig_robots_user_agent.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("ai_txt_disallow", DataType::Boolean, false)),
            Arc::new(sig_ai_txt_disallow.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("tdmrep_reservation", DataType::Int8, false)),
            Arc::new(sig_tdmrep_reservation.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("tdmrep_policy_url", DataType::Utf8, true)),
            Arc::new(sig_tdmrep_policy_url.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("aipref_usage_pref", DataType::Utf8, true)),
            Arc::new(sig_aipref_usage_pref.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("iptc_plus_dmi", DataType::Utf8, true)),
            Arc::new(sig_iptc_plus_dmi.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("c2pa_training_mining", DataType::Utf8, true)),
            Arc::new(sig_c2pa_training_mining.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("rsl_permits", DataType::Utf8, true)),
            Arc::new(sig_rsl_permits.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("liccium_tdmai_iscc", DataType::Utf8, true)),
            Arc::new(sig_liccium_tdmai_iscc.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("liccium_tdmai_allow", DataType::Boolean, true)),
            Arc::new(sig_liccium_tdmai_allow.finish()) as ArrayRef,
        ),
        (
            Arc::new(Field::new("cloudflare_ai_train", DataType::Utf8, true)),
            Arc::new(sig_cloudflare_ai_train.finish()) as ArrayRef,
        ),
    ]);

    let columns: Vec<ArrayRef> = vec![
        Arc::new(document_id.finish()),
        Arc::new(sha256.finish()),
        Arc::new(size_bytes.finish()),
        Arc::new(modality.finish()),
        Arc::new(mime_type.finish()),
        Arc::new(source_url.finish()),
        Arc::new(source_type.finish()),
        Arc::new(source_dataset_id.finish()),
        Arc::new(registered_domain.finish()),
        Arc::new(license_spdx.finish()),
        Arc::new(language.finish()),
        Arc::new(fetched_at.finish()),
        Arc::new(signals_struct),
        Arc::new(included.finish()),
        Arc::new(exclusion_reason.finish()),
        Arc::new(chunk_refs.finish()),
        Arc::new(input_ordinal.finish()),
        Arc::new(occurrence_index.finish()),
    ];

    RecordBatch::try_new(arrow_schema(), columns)
        .map_err(|e| AttestrumError::Internal(format!("record batch assembly: {e}")))
}

fn append_opt_str(builder: &mut StringBuilder, value: Option<&str>) {
    match value {
        Some(s) => builder.append_value(s),
        None => builder.append_null(),
    }
}

// ============================================================================
// Read path
// ============================================================================

/// Read a Parquet manifest from `path` and reconstruct the
/// [`ManifestEntry`] rows.
///
/// Single-threaded reader. Validates the on-disk schema matches
/// [`arrow_schema`] by attempting to downcast each column to its expected
/// concrete Arrow array type — a schema mismatch surfaces as
/// `AttestrumError::Internal`.
pub fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
    let file = File::open(path).map_err(AttestrumError::Io)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| AttestrumError::Internal(format!("parquet reader init: {e}")))?;
    let reader = builder
        .build()
        .map_err(|e| AttestrumError::Internal(format!("parquet reader build: {e}")))?;
    let mut out: Vec<ManifestEntry> = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|e| AttestrumError::Internal(format!("parquet batch: {e}")))?;
        record_batch_to_entries(&batch, &mut out)?;
    }
    Ok(out)
}

/// Read the file-level KeyValue metadata for a Parquet manifest. Returns the
/// pair `(schema_version, writer_profile)` from the
/// `attestrum.manifest.schema_version` and `attestrum.writer.profile` keys, or
/// errors if either is missing.
pub fn read_manifest_metadata(path: &Path) -> Result<(String, String)> {
    let file = File::open(path).map_err(AttestrumError::Io)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| AttestrumError::Internal(format!("parquet reader init: {e}")))?;
    let meta = builder.metadata().file_metadata();
    let kvs = meta.key_value_metadata().ok_or_else(|| {
        AttestrumError::Internal("manifest: no KeyValue metadata in file footer".to_string())
    })?;
    let mut schema_version = None;
    let mut writer_profile = None;
    for kv in kvs {
        if kv.key == KV_SCHEMA_VERSION {
            schema_version.clone_from(&kv.value);
        } else if kv.key == KV_WRITER_PROFILE {
            writer_profile.clone_from(&kv.value);
        }
    }
    Ok((
        schema_version.ok_or_else(|| {
            AttestrumError::Internal(format!("manifest: missing {KV_SCHEMA_VERSION} KeyValue"))
        })?,
        writer_profile.ok_or_else(|| {
            AttestrumError::Internal(format!("manifest: missing {KV_WRITER_PROFILE} KeyValue"))
        })?,
    ))
}

// ============================================================================
// Streaming I/O — constant-memory merge support
// ============================================================================

/// Row batch size for [`ManifestBatchReader`]. Small enough that many shard
/// readers buffered concurrently stay well within a runner's RAM, large enough
/// to amortize per-batch overhead. NOT a PROTECTED constant — it never affects
/// output bytes (the row sequence is what determines the file), only the
/// reader's working-set size.
const STREAM_BATCH_ROWS: usize = 8192;

/// Total row count of a Parquet manifest, read from the file footer only (no
/// row-group decode). Used to compute per-shard `input_ordinal` offsets before
/// a streaming merge begins, without materializing any rows.
pub fn manifest_row_count(path: &Path) -> Result<u64> {
    let file = File::open(path).map_err(AttestrumError::Io)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| AttestrumError::Internal(format!("parquet reader init: {e}")))?;
    let n = builder.metadata().file_metadata().num_rows();
    u64::try_from(n)
        .map_err(|_| AttestrumError::Internal(format!("manifest: negative row count {n}")))
}

/// A batched, constant-memory reader over a Parquet manifest. Yields
/// `Vec<ManifestEntry>` chunks of at most [`STREAM_BATCH_ROWS`] rows in on-disk
/// order — the same row order [`read_manifest`] returns (canonical
/// `(document_id, occurrence_index)` sort as written), without materializing
/// the whole manifest.
///
/// Used by `attestrum merge` to k-way-merge sharded manifests at ~100M-row
/// scale without holding every row in memory.
pub struct ManifestBatchReader {
    inner: ParquetRecordBatchReader,
}

impl ManifestBatchReader {
    /// Open `path` for batched reading. Validates only the Parquet footer
    /// eagerly; per-batch schema downcasts happen lazily in [`Iterator::next`].
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(AttestrumError::Io)?;
        let inner = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(|e| AttestrumError::Internal(format!("parquet reader init: {e}")))?
            .with_batch_size(STREAM_BATCH_ROWS)
            .build()
            .map_err(|e| AttestrumError::Internal(format!("parquet reader build: {e}")))?;
        Ok(Self { inner })
    }
}

impl Iterator for ManifestBatchReader {
    type Item = Result<Vec<ManifestEntry>>;

    fn next(&mut self) -> Option<Self::Item> {
        let batch = self.inner.next()?;
        Some(
            batch
                .map_err(|e| AttestrumError::Internal(format!("parquet batch: {e}")))
                .and_then(|b| {
                    let mut out = Vec::with_capacity(b.num_rows());
                    record_batch_to_entries(&b, &mut out)?;
                    Ok(out)
                }),
        )
    }
}

/// A streaming writer for Attestrum manifests. Accepts entries through repeated
/// [`ManifestWriter::write_entries`] calls and flushes a single Parquet file on
/// [`ManifestWriter::close`], using the PROTECTED [`writer_properties`].
///
/// **Byte-identity guarantee** (and why the internal buffering is load-bearing,
/// not incidental): the output is byte-identical to a single
/// [`write_manifest`] of the same row sequence *only* because this writer flushes
/// to the underlying `ArrowWriter` in batches of exactly `max_row_group_size`
/// rows. arrow-rs is NOT write-call-invariant: feeding it batches that straddle
/// a row-group boundary shifts a data-page boundary near the cut and changes the
/// bytes (observed as a 1-byte delta — see
/// `streaming_writer_byte_identical_to_one_shot_across_row_group_boundary`). By
/// making each `ArrowWriter::write` cover exactly one row group, every row group
/// sees the identical value sequence whether it arrived as its own batch here or
/// as a slice arrow split out of one giant `write_manifest` batch — so the
/// per-row-group encoding, and thus the whole file, matches.
///
/// Memory is bounded by one row group's worth of buffered entries
/// (`max_row_group_size`), independent of total manifest size — the property the
/// streaming merge needs at ~100M-row scale. The caller is responsible for the
/// canonical pipeline ordering (see [`write_manifest`]).
pub struct ManifestWriter {
    inner: ArrowWriter<File>,
    buf: Vec<ManifestEntry>,
    row_group_rows: usize,
}

impl ManifestWriter {
    /// Create the output file and initialize the PROTECTED Parquet writer.
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path).map_err(AttestrumError::Io)?;
        let props = writer_properties();
        let row_group_rows = props.max_row_group_size();
        let inner = ArrowWriter::try_new(file, arrow_schema(), Some(props))
            .map_err(|e| AttestrumError::Internal(format!("parquet writer init: {e}")))?;
        Ok(Self {
            inner,
            buf: Vec::new(),
            row_group_rows,
        })
    }

    /// Append a chunk of entries. Buffers internally and flushes a full row
    /// group to the Parquet writer whenever `max_row_group_size` rows have
    /// accumulated; the remainder is flushed by [`close`]. An empty slice is a
    /// no-op.
    ///
    /// [`close`]: ManifestWriter::close
    pub fn write_entries(&mut self, entries: &[ManifestEntry]) -> Result<()> {
        self.buf.extend_from_slice(entries);
        while self.buf.len() >= self.row_group_rows {
            let tail = self.buf.split_off(self.row_group_rows);
            let group = std::mem::replace(&mut self.buf, tail);
            self.flush_batch(&group)?;
        }
        Ok(())
    }

    fn flush_batch(&mut self, entries: &[ManifestEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let batch = entries_to_record_batch(entries)?;
        self.inner
            .write(&batch)
            .map_err(|e| AttestrumError::Internal(format!("parquet write batch: {e}")))?;
        Ok(())
    }

    /// Flush the final partial row group, finalize the Parquet footer, and
    /// close the file. Consumes the writer.
    pub fn close(mut self) -> Result<()> {
        let group = std::mem::take(&mut self.buf);
        self.flush_batch(&group)?;
        self.inner
            .close()
            .map_err(|e| AttestrumError::Internal(format!("parquet close: {e}")))?;
        Ok(())
    }
}

fn record_batch_to_entries(batch: &RecordBatch, out: &mut Vec<ManifestEntry>) -> Result<()> {
    let cols = batch.columns();
    if cols.len() != 18 {
        return Err(AttestrumError::Internal(format!(
            "manifest: expected 18 columns, got {}",
            cols.len()
        )));
    }
    let document_id = downcast_col::<FixedSizeBinaryArray>(&cols[0], "document_id")?;
    let sha256 = downcast_col::<FixedSizeBinaryArray>(&cols[1], "sha256")?;
    let size_bytes = downcast_col::<UInt64Array>(&cols[2], "size_bytes")?;
    let modality = downcast_col::<Int8Array>(&cols[3], "modality")?;
    let mime_type = downcast_col::<StringArray>(&cols[4], "mime_type")?;
    let source_url = downcast_col::<StringArray>(&cols[5], "source_url")?;
    let source_type = downcast_col::<Int8Array>(&cols[6], "source_type")?;
    let source_dataset_id = downcast_col::<StringArray>(&cols[7], "source_dataset_id")?;
    let registered_domain = downcast_col::<StringArray>(&cols[8], "registered_domain")?;
    let license_spdx = downcast_col::<StringArray>(&cols[9], "license_spdx")?;
    let language = downcast_col::<StringArray>(&cols[10], "language")?;
    let fetched_at = downcast_col::<Int64Array>(&cols[11], "fetched_at")?;
    let signals = downcast_col::<StructArray>(&cols[12], "signals")?;
    let included = downcast_col::<BooleanArray>(&cols[13], "included")?;
    let exclusion_reason = downcast_col::<StringArray>(&cols[14], "exclusion_reason")?;
    let chunk_refs = downcast_col::<ListArray>(&cols[15], "chunk_refs")?;
    let input_ordinal = downcast_col::<UInt64Array>(&cols[16], "input_ordinal")?;
    let occurrence_index = downcast_col::<UInt32Array>(&cols[17], "occurrence_index")?;

    // Pre-extract signals child columns (one downcast per child, reused per row)
    let sig_robots_disallow =
        struct_child::<BooleanArray>(signals, "robots_disallow", "signals.robots_disallow")?;
    let sig_robots_user_agent =
        struct_child::<StringArray>(signals, "robots_user_agent", "signals.robots_user_agent")?;
    let sig_ai_txt_disallow =
        struct_child::<BooleanArray>(signals, "ai_txt_disallow", "signals.ai_txt_disallow")?;
    let sig_tdmrep_reservation =
        struct_child::<Int8Array>(signals, "tdmrep_reservation", "signals.tdmrep_reservation")?;
    let sig_tdmrep_policy_url =
        struct_child::<StringArray>(signals, "tdmrep_policy_url", "signals.tdmrep_policy_url")?;
    let sig_aipref_usage_pref =
        struct_child::<StringArray>(signals, "aipref_usage_pref", "signals.aipref_usage_pref")?;
    let sig_iptc_plus_dmi =
        struct_child::<StringArray>(signals, "iptc_plus_dmi", "signals.iptc_plus_dmi")?;
    let sig_c2pa_training_mining = struct_child::<StringArray>(
        signals,
        "c2pa_training_mining",
        "signals.c2pa_training_mining",
    )?;
    let sig_rsl_permits =
        struct_child::<StringArray>(signals, "rsl_permits", "signals.rsl_permits")?;
    let sig_liccium_tdmai_iscc =
        struct_child::<StringArray>(signals, "liccium_tdmai_iscc", "signals.liccium_tdmai_iscc")?;
    let sig_liccium_tdmai_allow = struct_child::<BooleanArray>(
        signals,
        "liccium_tdmai_allow",
        "signals.liccium_tdmai_allow",
    )?;
    let sig_cloudflare_ai_train = struct_child::<StringArray>(
        signals,
        "cloudflare_ai_train",
        "signals.cloudflare_ai_train",
    )?;

    out.reserve(batch.num_rows());
    for i in 0..batch.num_rows() {
        let mut doc_id = [0u8; 32];
        doc_id.copy_from_slice(document_id.value(i));
        let mut sha = [0u8; 32];
        sha.copy_from_slice(sha256.value(i));
        let chunk_list = if chunk_refs.is_null(i) {
            None
        } else {
            let list_value = chunk_refs.value(i);
            let inner = list_value
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .ok_or_else(|| {
                    AttestrumError::Internal(
                        "manifest: chunk_refs inner is not FixedSizeBinaryArray".to_string(),
                    )
                })?;
            let mut v = Vec::with_capacity(inner.len());
            for j in 0..inner.len() {
                let mut h = [0u8; 32];
                h.copy_from_slice(inner.value(j));
                v.push(h);
            }
            Some(v)
        };
        let entry = ManifestEntry {
            document_id: doc_id,
            sha256: sha,
            size_bytes: size_bytes.value(i),
            modality: modality_from_code(modality.value(i))?,
            mime_type: opt_string(mime_type, i),
            source_url: opt_string(source_url, i),
            source_type: if source_type.is_null(i) {
                None
            } else {
                Some(source_type_from_code(source_type.value(i))?)
            },
            source_dataset_id: opt_string(source_dataset_id, i),
            registered_domain: opt_string(registered_domain, i),
            license_spdx: opt_string(license_spdx, i),
            language: opt_string(language, i),
            fetched_at: if fetched_at.is_null(i) {
                None
            } else {
                Some(fetched_at.value(i))
            },
            signals: ManifestSignals {
                robots_disallow: sig_robots_disallow.value(i),
                robots_user_agent: opt_string(sig_robots_user_agent, i),
                ai_txt_disallow: sig_ai_txt_disallow.value(i),
                tdmrep_reservation: sig_tdmrep_reservation.value(i),
                tdmrep_policy_url: opt_string(sig_tdmrep_policy_url, i),
                aipref_usage_pref: opt_string(sig_aipref_usage_pref, i),
                iptc_plus_dmi: opt_string(sig_iptc_plus_dmi, i),
                c2pa_training_mining: opt_string(sig_c2pa_training_mining, i),
                rsl_permits: opt_string(sig_rsl_permits, i),
                liccium_tdmai_iscc: opt_string(sig_liccium_tdmai_iscc, i),
                liccium_tdmai_allow: if sig_liccium_tdmai_allow.is_null(i) {
                    None
                } else {
                    Some(sig_liccium_tdmai_allow.value(i))
                },
                cloudflare_ai_train: opt_string(sig_cloudflare_ai_train, i),
            },
            included: included.value(i),
            exclusion_reason: opt_string(exclusion_reason, i),
            chunk_refs: chunk_list,
            input_ordinal: input_ordinal.value(i),
            occurrence_index: occurrence_index.value(i),
        };
        out.push(entry);
    }
    Ok(())
}

fn downcast_col<'a, A: Array + 'static>(col: &'a ArrayRef, name: &str) -> Result<&'a A> {
    col.as_any().downcast_ref::<A>().ok_or_else(|| {
        AttestrumError::Internal(format!(
            "manifest: column {name} has unexpected Arrow array type"
        ))
    })
}

fn struct_child<'a, A: Array + 'static>(
    s: &'a StructArray,
    field_name: &str,
    diag_name: &str,
) -> Result<&'a A> {
    s.column_by_name(field_name)
        .ok_or_else(|| {
            AttestrumError::Internal(format!(
                "manifest: signals STRUCT missing field {field_name}"
            ))
        })?
        .as_any()
        .downcast_ref::<A>()
        .ok_or_else(|| {
            AttestrumError::Internal(format!(
                "manifest: column {diag_name} has unexpected Arrow array type"
            ))
        })
}

fn opt_string(arr: &StringArray, i: usize) -> Option<String> {
    if arr.is_null(i) {
        None
    } else {
        Some(arr.value(i).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modality_code_bijection() {
        for m in [
            Modality::Text,
            Modality::Image,
            Modality::Audio,
            Modality::Video,
            Modality::Pdf,
            Modality::Other,
        ] {
            let c = modality_to_code(m);
            let back = modality_from_code(c).expect("known code");
            assert_eq!(back, m, "modality bijection failed for {m:?}");
        }
    }

    #[test]
    fn modality_unknown_code_rejected() {
        for bad in [-1i8, 6, 7, 42, i8::MAX, i8::MIN] {
            assert!(modality_from_code(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn source_type_code_bijection() {
        for s in [
            SourceType::Crawl,
            SourceType::PublicDataset,
            SourceType::PrivateLicensed,
            SourceType::User,
            SourceType::Synthetic,
            SourceType::Other,
        ] {
            let c = source_type_to_code(s);
            let back = source_type_from_code(c).expect("known code");
            assert_eq!(back, s, "source_type bijection failed for {s:?}");
        }
    }

    #[test]
    fn schema_has_18_top_level_fields() {
        let s = arrow_schema();
        assert_eq!(s.fields().len(), 18);
    }

    #[test]
    fn schema_signals_struct_has_12_children() {
        let s = arrow_schema();
        let signals_field = s.field_with_name("signals").unwrap();
        match signals_field.data_type() {
            DataType::Struct(children) => assert_eq!(children.len(), 12),
            other => panic!("signals should be Struct, got {other:?}"),
        }
    }

    #[test]
    fn writer_properties_pin_critical_settings() {
        let p = writer_properties();
        assert_eq!(p.writer_version(), WriterVersion::PARQUET_1_0);
        assert_eq!(
            p.compression(&"document_id".into()),
            Compression::ZSTD(ZstdLevel::try_new(3).unwrap())
        );
        assert!(!p.dictionary_enabled(&"document_id".into()));
        assert_eq!(
            p.statistics_enabled(&"document_id".into()),
            EnabledStatistics::None
        );
        assert_eq!(p.max_row_group_size(), 1_000_000);
        assert_eq!(p.created_by(), CREATED_BY);
        let kvs = p.key_value_metadata().expect("key value metadata present");
        let kv_keys: Vec<&str> = kvs.iter().map(|kv| kv.key.as_str()).collect();
        assert!(kv_keys.contains(&KV_SCHEMA_VERSION));
        assert!(kv_keys.contains(&KV_WRITER_PROFILE));
    }

    // ------------------------------------------------------------------------
    // Streaming I/O
    // ------------------------------------------------------------------------

    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "attestrum-io-test-{name}-{}-{n}.parquet",
            std::process::id()
        ));
        p
    }

    /// A lightweight entry whose `document_id` encodes `seed` — keeps the >1M
    /// byte-invariance test's working set small without affecting the writer
    /// codepaths under test.
    fn min_entry(seed: u64) -> ManifestEntry {
        let mut document_id = [0u8; 32];
        document_id[..8].copy_from_slice(&seed.to_le_bytes());
        ManifestEntry {
            document_id,
            sha256: [0u8; 32],
            size_bytes: seed,
            modality: Modality::Text,
            mime_type: None,
            source_url: None,
            source_type: None,
            source_dataset_id: None,
            registered_domain: None,
            license_spdx: None,
            language: None,
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
            chunk_refs: None,
            input_ordinal: seed,
            occurrence_index: 0,
        }
    }

    #[test]
    fn streaming_reader_yields_same_rows_as_read_manifest() {
        let path = tmp_path("stream-read");
        let entries: Vec<ManifestEntry> = (0..1000).map(min_entry).collect();
        write_manifest(&path, &entries).expect("write");

        let bulk = read_manifest(&path).expect("read_manifest");
        let mut streamed: Vec<ManifestEntry> = Vec::new();
        for batch in ManifestBatchReader::open(&path).expect("open stream reader") {
            streamed.extend(batch.expect("batch"));
        }
        std::fs::remove_file(&path).ok();

        assert_eq!(streamed.len(), 1000);
        assert_eq!(bulk, streamed, "streaming reader must yield identical rows");
    }

    #[test]
    fn manifest_row_count_reads_footer() {
        let path = tmp_path("rowcount");
        let entries: Vec<ManifestEntry> = (0..777).map(min_entry).collect();
        write_manifest(&path, &entries).expect("write");
        let n = manifest_row_count(&path).expect("row count");
        std::fs::remove_file(&path).ok();
        assert_eq!(n, 777);
    }

    /// The load-bearing byte-identity assumption behind the streaming merge:
    /// chunking writes across many `write_entries` calls produces bytes
    /// identical to one giant `write_manifest`, because Parquet row-group and
    /// data-page boundaries are accumulation-driven, not write-call-driven.
    /// `n > max_row_group_size` so the file spans two row groups — proving the
    /// invariance across BOTH a row-group boundary and the many intra-group
    /// data-page boundaries.
    #[test]
    fn streaming_writer_byte_identical_to_one_shot_across_row_group_boundary() {
        let n: u64 = 1_000_000 + 50_000;
        let entries: Vec<ManifestEntry> = (0..n).map(min_entry).collect();

        let one_shot = tmp_path("oneshot");
        write_manifest(&one_shot, &entries).expect("one-shot write");

        let streamed = tmp_path("streamed");
        let mut w = ManifestWriter::create(&streamed).expect("create stream writer");
        for chunk in entries.chunks(STREAM_BATCH_ROWS) {
            w.write_entries(chunk).expect("write chunk");
        }
        w.close().expect("close");

        let a = std::fs::read(&one_shot).expect("read one-shot");
        let b = std::fs::read(&streamed).expect("read streamed");
        std::fs::remove_file(&one_shot).ok();
        std::fs::remove_file(&streamed).ok();

        assert_eq!(
            a.len(),
            b.len(),
            "byte length differs: one-shot {} vs streamed {}",
            a.len(),
            b.len()
        );
        assert!(
            a == b,
            "streaming writer output is not byte-identical to one-shot write_manifest"
        );
    }
}
