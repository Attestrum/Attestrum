//! fineweb-edu Parquet → sealed corpus, the programmatic core of the
//! fineweb-edu seal generator (`examples/seal-fineweb-edu.rs`).
//!
//! Like the dolly and pg19 cores, this file lives under `examples/fineweb_edu/`
//! (a *subdirectory*, so Cargo does NOT treat it as an example binary target)
//! and is included two ways:
//!   - `examples/seal-fineweb-edu.rs` (the runnable generator);
//!   - `tests/fineweb_edu_seal.rs`, so the `#[cfg(test)]` block below runs
//!     under `cargo test --workspace`.
//!
//! Reading Parquet needs `arrow` + `parquet` (dev-dependencies; the library
//! stays Parquet-free). The fixture tests write zstd-compressed shards, so the
//! workspace parquet `zstd` feature's decode path is exercised in CI alongside
//! the dolly/WikiText `snap` path.
//!
//! **Leaf contract** (`docs/diagrams/lookback/fineweb10bt-seal-pipeline.md`):
//! one parquet row = one leaf; the sealed bytes are the row's `text` column
//! bytes EXACTLY — no render, no normalization, no added newline. Metadata
//! columns are not sealed. `source_uri` is the row's own `id` (a `urn:uuid`,
//! globally unique and shard-invariant), so a leaf's identity is independent
//! of which matrix shard sealed it — the property the sharded-merge rung
//! relies on. The PROTECTED `attestrum-fingerprint` normalization
//! (CLAUDE.md §4) is untouched.
//!
//! **Determinism contract** (extends `sprint-3-corpus`): shards are read in
//! sorted filename order, rows in file order; the same shard set sealed twice
//! yields a byte-identical `manifest.parquet` + Merkle root. Sealing the
//! shards separately and merging (the CI matrix topology) yields the same
//! root as sealing them together — asserted by the split-vs-whole test below.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use arrow::array::{Array, StringArray};
use attestrum_cas::CasStore;
use attestrum_core::{hex, BuildContext, Modality, SourceType};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildError, BuildOutput, ContentSource, CorpusEntry};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

/// SPDX license of HuggingFaceFW/fineweb-edu (Open Data Commons Attribution;
/// use is additionally subject to the CommonCrawl Terms of Use — disclosed in
/// `docs/lookback/fineweb10bt-attribution.md`).
const FINEWEB_LICENSE_SPDX: &str = "ODC-By-1.0";
/// Dataset id recorded in each leaf's `source_dataset_id`.
const FINEWEB_DATASET_ID: &str = "fineweb-edu";

/// One fineweb-edu row, reduced to the columns the seal consumes. The other
/// metadata columns (`dump`, `url`, scores, …) are deliberately not read —
/// they are not part of the sealed leaf.
pub struct FinewebRow {
    /// Sealed bytes: this string's UTF-8 bytes, exactly.
    pub text: String,
    /// The row's upstream `id` (`urn:uuid`), used verbatim as `source_uri`.
    pub id: String,
    /// Upstream `language` tag (`None` when the cell is null).
    pub language: Option<String>,
}

/// Errors the seal generator can surface. The protected pipeline error
/// (`BuildError`) flows through `Build`; everything upstream of `build_corpus`
/// (Parquet I/O, schema shape) gets its own variant carrying the offending path.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("parquet error reading {path}: {source}")]
    Parquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("shard {path} is missing a Utf8 '{column}' column")]
    NoColumn { path: PathBuf, column: &'static str },

    #[error(transparent)]
    Build(#[from] BuildError),
}

/// Downcast a named column of a record batch to a `StringArray`, mapping a
/// missing-or-non-Utf8 column to [`SealError::NoColumn`] carrying the shard path.
fn string_column<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &'static str,
    path: &Path,
) -> Result<&'a StringArray, SealError> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| SealError::NoColumn {
            path: path.to_path_buf(),
            column: name,
        })
}

/// Read the `text` / `id` / `language` columns from one Parquet shard, in file
/// order, into [`FinewebRow`]s. Null `text` / `id` cells become empty strings
/// (defensive — fineweb-edu publishes neither as nullable); a null `language`
/// becomes `None`.
fn read_rows(path: &Path) -> Result<Vec<FinewebRow>, SealError> {
    let file = File::open(path).map_err(|source| SealError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .and_then(|b| b.build())
        .map_err(|source| SealError::Parquet {
            path: path.to_path_buf(),
            source,
        })?;

    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|source| SealError::Parquet {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        let text = string_column(&batch, "text", path)?;
        let id = string_column(&batch, "id", path)?;
        let language = string_column(&batch, "language", path)?;
        let cell = |a: &StringArray, i: usize| -> String {
            if a.is_null(i) {
                String::new()
            } else {
                a.value(i).to_string()
            }
        };
        for i in 0..batch.num_rows() {
            rows.push(FinewebRow {
                text: cell(text, i),
                id: cell(id, i),
                language: if language.is_null(i) {
                    None
                } else {
                    Some(language.value(i).to_string())
                },
            });
        }
    }
    Ok(rows)
}

/// List the `*.parquet` shards in `dir`, sorted by filename (the deterministic
/// shard order the manifest's input ordinals derive from).
fn sorted_shards(dir: &Path) -> Result<Vec<PathBuf>, SealError> {
    let mut shards: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|source| SealError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| SealError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
            shards.push(path);
        }
    }
    shards.sort();
    Ok(shards)
}

/// Read every shard in `dir` into rows, in (shard filename, row) order.
pub fn rows_from_dir(dir: &Path) -> Result<Vec<FinewebRow>, SealError> {
    let mut rows: Vec<FinewebRow> = Vec::new();
    for shard in sorted_shards(dir)? {
        rows.extend(read_rows(&shard)?);
    }
    Ok(rows)
}

/// Map fineweb rows to pipeline corpus entries. Each row is one leaf; the
/// sealed bytes are the `text` column bytes exactly. The `source_uri` is the
/// row's own upstream `id` — shard-invariant, so the entry set (and therefore
/// the merged Merkle root) does not depend on how rows were partitioned
/// across matrix jobs.
pub fn rows_to_entries(rows: Vec<FinewebRow>) -> Vec<CorpusEntry> {
    rows.into_iter()
        .map(|r| CorpusEntry {
            source_uri: r.id,
            content: ContentSource::Bytes(r.text.into_bytes()),
            modality: Modality::Text,
            mime_type: Some("text/plain".to_string()),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some(FINEWEB_DATASET_ID.to_string()),
            registered_domain: None,
            license_spdx: Some(FINEWEB_LICENSE_SPDX.to_string()),
            language: r.language,
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
        })
        .collect()
}

/// Seal `entries` into `output_dir`: a CAS under `<output_dir>/.attestrum/` and
/// a `manifest.parquet` + `merkle.root` under `<output_dir>/.attestrum/manifests/`.
/// Mirrors the dolly/pg19 seals so the output is a drop-in for `attestrum merge`
/// inputs and the publish path's `--merkle-root` arg.
pub fn seal(
    entries: &[CorpusEntry],
    output_dir: &Path,
    epoch: i64,
) -> Result<BuildOutput, SealError> {
    let cas = CasStore::new(output_dir.join(".attestrum")).map_err(|source| SealError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;
    let ctx = BuildContext::new(output_dir.to_path_buf(), epoch);
    let manifest_dir = output_dir.join(".attestrum").join("manifests");
    let output = build_corpus(&ctx, &cas, entries, &manifest_dir)?;

    let merkle_root_path = manifest_dir.join("merkle.root");
    fs::write(
        &merkle_root_path,
        format!("{}\n", hex::encode_32(&output.merkle_root)),
    )
    .map_err(|source| SealError::Io {
        path: merkle_root_path,
        source,
    })?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use attestrum_manifest::{
        assign_input_ordinals, assign_occurrence_indices, read_manifest, sort_entries,
    };
    use parquet::arrow::ArrowWriter;
    use parquet::basic::{Compression, ZstdLevel};
    use parquet::file::properties::WriterProperties;

    /// Write a fineweb-shaped Parquet shard: the three sealed-relevant columns
    /// plus `dump` and `url` metadata columns the seal must ignore. Compressed
    /// with zstd to exercise the workspace parquet `zstd` decode feature.
    fn write_shard(path: &Path, rows: &[(&str, &str, &str)]) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("text", DataType::Utf8, false),
            Field::new("id", DataType::Utf8, false),
            Field::new("dump", DataType::Utf8, false),
            Field::new("url", DataType::Utf8, false),
            Field::new("language", DataType::Utf8, true),
        ]));
        let text = StringArray::from(rows.iter().map(|r| r.0).collect::<Vec<_>>());
        let id = StringArray::from(rows.iter().map(|r| r.1).collect::<Vec<_>>());
        let dump = StringArray::from(vec!["CC-MAIN-2024-10"; rows.len()]);
        let url = StringArray::from(vec!["https://example.com/page"; rows.len()]);
        let language = StringArray::from(rows.iter().map(|r| r.2).collect::<Vec<_>>());
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(text),
                Arc::new(id),
                Arc::new(dump),
                Arc::new(url),
                Arc::new(language),
            ],
        )
        .unwrap();
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::try_new(3).unwrap()))
            .build();
        let file = File::create(path).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema, Some(props)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
    }

    fn fixture_rows() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "Photosynthesis converts light energy into chemical energy.\n",
                "<urn:uuid:0198d479-aef9-4d54-9f5d-000000000001>",
                "en",
            ),
            // No trailing newline — the exact-bytes contract must not add one.
            (
                "The mitochondrion is the powerhouse of the cell.",
                "<urn:uuid:0198d479-aef9-4d54-9f5d-000000000002>",
                "en",
            ),
            (
                "Pythagoras: a\u{00b2} + b\u{00b2} = c\u{00b2} for right triangles.",
                "<urn:uuid:0198d479-aef9-4d54-9f5d-000000000003>",
                "en",
            ),
            (
                "Rivers erode their banks over geological time.",
                "<urn:uuid:0198d479-aef9-4d54-9f5d-000000000004>",
                "en",
            ),
        ]
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "attestrum-fineweb-test-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_rows_and_maps_fixed_provenance() {
        let input = unique_dir("rows-input");
        write_shard(&input.join("000_00000.parquet"), &fixture_rows());

        let rows = rows_from_dir(&input).unwrap();
        assert_eq!(rows.len(), 4);

        let entries = rows_to_entries(rows);
        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries[0].source_uri,
            "<urn:uuid:0198d479-aef9-4d54-9f5d-000000000001>"
        );
        assert!(entries.iter().all(|e| e.modality == Modality::Text));
        assert!(entries
            .iter()
            .all(|e| e.source_dataset_id.as_deref() == Some("fineweb-edu")));
        assert!(entries
            .iter()
            .all(|e| e.license_spdx.as_deref() == Some("ODC-By-1.0")));
        assert!(entries.iter().all(|e| e.language.as_deref() == Some("en")));
        // Exact-bytes contract: text bytes verbatim, no added newline.
        if let ContentSource::Bytes(b) = &entries[1].content {
            assert_eq!(b, b"The mitochondrion is the powerhouse of the cell.");
        } else {
            panic!("expected Bytes content");
        }

        let _ = fs::remove_dir_all(&input);
    }

    #[test]
    fn seal_is_deterministic_over_fixture_parquet() {
        let input = unique_dir("det-input");
        write_shard(&input.join("000_00000.parquet"), &fixture_rows());

        let entries = rows_to_entries(rows_from_dir(&input).unwrap());
        assert!(!entries.is_empty());

        let ws1 = unique_dir("det-ws1");
        let ws2 = unique_dir("det-ws2");
        let out1 = seal(&entries, &ws1, 0).unwrap();
        let out2 = seal(&entries, &ws2, 0).unwrap();

        assert_eq!(out1.merkle_root, out2.merkle_root, "Merkle root must match");
        assert_eq!(out1.leaf_count, entries.len());
        assert_eq!(
            fs::read(&out1.manifest_path).unwrap(),
            fs::read(&out2.manifest_path).unwrap(),
            "manifest.parquet bytes must be identical"
        );

        // Exact-bytes leaf contract: the CAS object for a known row equals the
        // text column bytes, hash-addressed by those bytes' own BLAKE3.
        let text_bytes: &[u8] = b"The mitochondrion is the powerhouse of the cell.";
        let digest = hex::encode_32(blake3::hash(text_bytes).as_bytes());
        let cas_path = ws1
            .join(".attestrum")
            .join("cas")
            .join("blake3")
            .join(&digest[0..2])
            .join(&digest[2..4])
            .join(format!("{digest}.bin"));
        assert_eq!(
            fs::read(&cas_path).unwrap(),
            text_bytes,
            "CAS object must be the exact text column bytes"
        );

        // merkle.root sibling: 64 lowercase hex + newline, identical across runs.
        let root_file_1 = ws1.join(".attestrum").join("manifests").join("merkle.root");
        let root_file_2 = ws2.join(".attestrum").join("manifests").join("merkle.root");
        let root_text_1 = fs::read_to_string(&root_file_1).unwrap();
        assert_eq!(
            root_text_1,
            format!("{}\n", hex::encode_32(&out1.merkle_root)),
            "merkle.root must be 64 lowercase hex + newline matching the manifest root"
        );
        assert_eq!(
            root_text_1,
            fs::read_to_string(&root_file_2).unwrap(),
            "merkle.root must be byte-identical across deterministic runs"
        );

        for d in [&input, &ws1, &ws2] {
            let _ = fs::remove_dir_all(d);
        }
    }

    /// The sharding contract through THIS example's leaves: sealing all rows
    /// at once vs sealing two halves separately and merging their manifests
    /// (the same `assign_input_ordinals` → `assign_occurrence_indices` →
    /// `sort_entries` global passes `attestrum merge` runs) yields the same
    /// Merkle root. This is the property the 14-shard CI matrix relies on.
    #[test]
    fn split_vs_whole_seal_yields_identical_merkle_root() {
        let all = fixture_rows();
        let (first, second) = all.split_at(2);

        let whole_in = unique_dir("svw-whole-in");
        write_shard(&whole_in.join("000_00000.parquet"), &all);
        let ws_whole = unique_dir("svw-whole-ws");
        let whole = seal(
            &rows_to_entries(rows_from_dir(&whole_in).unwrap()),
            &ws_whole,
            0,
        )
        .unwrap();

        let mut shard_manifests = Vec::new();
        for (n, half) in [first, second].into_iter().enumerate() {
            let half_in = unique_dir(&format!("svw-half{n}-in"));
            write_shard(&half_in.join("000_00000.parquet"), half);
            let ws = unique_dir(&format!("svw-half{n}-ws"));
            seal(&rows_to_entries(rows_from_dir(&half_in).unwrap()), &ws, 0).unwrap();
            shard_manifests.push(
                ws.join(".attestrum")
                    .join("manifests")
                    .join("manifest.parquet"),
            );
        }

        let mut merged = Vec::new();
        for m in &shard_manifests {
            merged.extend(read_manifest(m).unwrap());
        }
        assign_input_ordinals(&mut merged);
        assign_occurrence_indices(&mut merged);
        sort_entries(&mut merged);
        let leaves: Vec<[u8; 32]> = merged.iter().map(|r| r.document_id).collect();
        let merged_root = attestrum_merkle::merkle_root(&leaves);

        assert_eq!(
            whole.merkle_root, merged_root,
            "split-then-merged root must equal the whole-seal root"
        );
    }

    #[test]
    fn missing_dir_garbage_parquet_and_missing_column_are_errors() {
        // Io: input dir does not exist.
        let gone = std::env::temp_dir().join(format!(
            "attestrum-fineweb-test-{}-does-not-exist",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&gone);
        assert!(matches!(rows_from_dir(&gone), Err(SealError::Io { .. })));

        // Parquet: a .parquet file that isn't parquet.
        let garbage = unique_dir("err-garbage");
        fs::write(garbage.join("000_00000.parquet"), b"not parquet at all").unwrap();
        assert!(matches!(
            rows_from_dir(&garbage),
            Err(SealError::Parquet { .. })
        ));

        // NoColumn: a real parquet shard without a `text` column.
        let no_text = unique_dir("err-no-text");
        {
            let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
            let id = StringArray::from(vec!["<urn:uuid:x>"]);
            let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(id)]).unwrap();
            let file = File::create(no_text.join("000_00000.parquet")).unwrap();
            let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
            writer.write(&batch).unwrap();
            writer.close().unwrap();
        }
        assert!(matches!(
            rows_from_dir(&no_text),
            Err(SealError::NoColumn { column: "text", .. })
        ));

        for d in [&garbage, &no_text] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
