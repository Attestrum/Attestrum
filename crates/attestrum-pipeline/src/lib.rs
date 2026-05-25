//! `attestrum-pipeline` — the end-to-end deterministic build pipeline.
//!
//! Wires Sprint 1 signal types + Sprint 2 [`attestrum_cas::stream_hash`] +
//! [`attestrum_cas::CasStore`] + Sprint 3 [`attestrum_manifest`] writer + Sprint 2
//! [`attestrum_merkle::merkle_root`] into a single `build_corpus` call. The
//! pipeline is a sequential prelude (deterministic input load) → a Rayon
//! `par_iter().fold(...).reduce(...)` parallel hash + CAS-put stage → a
//! sequential epilogue (sort + merkle + manifest write).
//!
//! See `docs/diagrams/sprint-3/rayon-pipeline.md` (flowchart) and
//! `docs/diagrams/sprint-3/cas-write-path.md` (sequenceDiagram) for the
//! canonical behaviour. Both diagrams flip to `source_of_truth: code` in
//! this commit (Sprint 3 E4).
//!
//! **Determinism contract**: same `[CorpusEntry]` input → same
//! `manifest.parquet` bytes + same Merkle root, byte-identical across all
//! four CI matrix targets (extended in E8). The Rayon work-stealing order
//! is non-deterministic but the final output is deterministic because:
//!
//! 1. Each worker stamps its row's `input_ordinal` from the
//!    `par_iter().enumerate()` index at construction time, so the value
//!    survives any reduce order.
//! 2. After the reduce we `sort_by_key(input_ordinal)` to restore canonical
//!    input order, then call [`attestrum_manifest::assign_occurrence_indices`]
//!    (which walks in input-order to assign per-digest rank), then
//!    [`attestrum_manifest::sort_entries`] for the canonical on-disk order.
//! 3. The Merkle leaves are extracted from the sorted manifest, so their
//!    order is the canonical sort order — `merkle_root` is then a pure
//!    function of the corpus as a multiset.
//!
//! **No `Mutex<Vec>`**. The E3 cross-check (founder-conducted via ChatGPT
//! 2026-05-24, responses preserved at `~/Downloads/attestrum-e3/`) flagged a
//! shared-mutex accumulator as the most-cited anti-pattern: it serialises
//! every worker push and becomes the throughput bottleneck under
//! contention with fast-hashing small docs. Rayon's `fold + reduce` builds
//! a per-worker `Vec` with no inter-worker synchronisation, then merges in
//! O(N) once at the end.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use attestrum_cas::{stream_hash, CasStore, StreamHash};
use attestrum_core::{BuildContext, Modality, SourceType};
use attestrum_manifest::{
    assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry, ManifestSignals,
};
use attestrum_merkle::merkle_root;
use rayon::prelude::*;
use thiserror::Error;

/// Where the bytes for a single corpus entry come from.
///
/// `Path` is the common case: the file lives on disk and is read fully
/// into memory once for both hashing and CAS-put (one syscall pair). The
/// `Bytes` variant is the in-memory case used by tests and any caller that
/// has already materialised the content.
///
/// A `Reader` variant was considered but rejected: a one-shot `io::Read`
/// can be fed to `stream_hash` (consuming it) OR to `CasStore::put` (which
/// needs `&[u8]`), not both. Callers with a reader should buffer it into
/// `Bytes(Vec<u8>)` themselves.
#[derive(Debug, Clone)]
pub enum ContentSource {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// One input row for the pipeline. Carries the caller-supplied signal /
/// provenance metadata that `build_corpus` cannot derive from content
/// alone, plus the bytes source. `input_ordinal` and `occurrence_index`
/// are NOT here — the pipeline assigns them.
#[derive(Debug, Clone)]
pub struct CorpusEntry {
    pub source_uri: String,
    pub content: ContentSource,
    pub modality: Modality,
    pub mime_type: Option<String>,
    pub source_type: Option<SourceType>,
    pub source_dataset_id: Option<String>,
    pub registered_domain: Option<String>,
    pub license_spdx: Option<String>,
    pub language: Option<String>,
    pub fetched_at: Option<i64>,
    pub signals: ManifestSignals,
    pub included: bool,
    pub exclusion_reason: Option<String>,
}

/// Summary of one `build_corpus` invocation. The `manifest.parquet` file
/// is at `manifest_path`; the Merkle root is over the sorted BLAKE3
/// digests in canonical on-disk order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    pub merkle_root: [u8; 32],
    pub manifest_path: PathBuf,
    pub leaf_count: usize,
    pub total_bytes: u64,
}

/// Errors `build_corpus` can return. `Io` carries the offending
/// `source_uri` so the operator knows which corpus entry failed;
/// everything else flows through [`attestrum_core::AttestrumError`] via
/// `Manifest`.
#[derive(Error, Debug)]
pub enum BuildError {
    #[error("io error reading source {source_uri}: {source}")]
    Io {
        source_uri: String,
        #[source]
        source: io::Error,
    },

    #[error("output_dir prepare failed at {path}: {source}")]
    OutputDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("manifest write failed: {0}")]
    Manifest(#[from] attestrum_core::AttestrumError),
}

/// Build a corpus end-to-end. Hashes every entry, lands it in `cas`,
/// constructs a deterministic `manifest.parquet` in `output_dir`, computes
/// the multiset Merkle root over the sorted leaves, and returns the
/// summary. `output_dir` is created if missing.
///
/// `ctx` is currently unused inside the pipeline (no timestamp injection
/// at this stage) but is taken by reference so future sprints can wire
/// `ctx.source_date_epoch` into derived fields without an API break.
pub fn build_corpus(
    ctx: &BuildContext,
    cas: &CasStore,
    entries: &[CorpusEntry],
    output_dir: &Path,
) -> Result<BuildOutput, BuildError> {
    // Silence the unused-warning until a later sprint wires ctx in.
    let _ = ctx;

    // Parallel phase: each worker hashes one entry, atomically lands the
    // bytes in CAS, and builds the ManifestEntry. input_ordinal is
    // stamped from the enumerate index AT CONSTRUCTION so it survives any
    // reduce order; assign_occurrence_indices runs later in the epilogue.
    //
    // fold builds per-worker Vec<ManifestEntry> accumulators with no
    // inter-worker synchronisation. reduce merges them at the end via
    // Vec::append (O(total rows), no rehashing). Result short-circuiting:
    // any Err from a worker propagates upward; later workers' completed
    // CAS writes remain on disk (CAS is append-only), but the manifest is
    // never written, so no partial sealed artifact escapes.
    let rows: Vec<ManifestEntry> = entries
        .par_iter()
        .enumerate()
        .fold(
            || Ok::<Vec<ManifestEntry>, BuildError>(Vec::new()),
            |acc, (i, entry)| {
                let mut acc = acc?;
                acc.push(build_row(i as u64, entry, cas)?);
                Ok(acc)
            },
        )
        .reduce(
            || Ok(Vec::new()),
            |a, b| {
                let mut a = a?;
                let mut b = b?;
                a.append(&mut b);
                Ok(a)
            },
        )?;

    // Epilogue (sequential, deterministic):
    //   1. Restore input order via input_ordinal (Rayon reduce order is
    //      arbitrary).
    //   2. assign_occurrence_indices walks in input order to set per-digest
    //      rank.
    //   3. sort_entries to canonical (document_id, occurrence_index) order.
    //   4. Extract digests in that order; compute Merkle root.
    //   5. Write the manifest to <output_dir>/manifest.parquet.
    let mut rows = rows;
    rows.sort_by_key(|r| r.input_ordinal);
    assign_occurrence_indices(&mut rows);
    sort_entries(&mut rows);

    let leaves: Vec<[u8; 32]> = rows.iter().map(|r| r.document_id).collect();
    let root = merkle_root(&leaves);

    fs::create_dir_all(output_dir).map_err(|source| BuildError::OutputDir {
        path: output_dir.to_path_buf(),
        source,
    })?;
    let manifest_path = output_dir.join("manifest.parquet");
    write_manifest(&manifest_path, &rows)?;

    let total_bytes = rows.iter().map(|r| r.size_bytes).sum();
    Ok(BuildOutput {
        merkle_root: root,
        manifest_path,
        leaf_count: rows.len(),
        total_bytes,
    })
}

/// Per-worker step: read the entry's bytes, compute the stream hash, land
/// the bytes in CAS, and assemble the ManifestEntry. `input_ordinal` is
/// stamped here from the parallel enumerate index.
fn build_row(
    input_ordinal: u64,
    entry: &CorpusEntry,
    cas: &CasStore,
) -> Result<ManifestEntry, BuildError> {
    let (hash, bytes) = read_and_hash(entry)?;
    cas.put(&hash.blake3, &bytes)
        .map_err(|source| BuildError::Io {
            source_uri: entry.source_uri.clone(),
            source,
        })?;

    Ok(ManifestEntry {
        document_id: hash.blake3,
        sha256: hash.sha256,
        size_bytes: hash.size_bytes,
        modality: entry.modality,
        mime_type: entry.mime_type.clone(),
        source_url: Some(entry.source_uri.clone()),
        source_type: entry.source_type,
        source_dataset_id: entry.source_dataset_id.clone(),
        registered_domain: entry.registered_domain.clone(),
        license_spdx: entry.license_spdx.clone(),
        language: entry.language.clone(),
        fetched_at: entry.fetched_at,
        signals: entry.signals.clone(),
        included: entry.included,
        exclusion_reason: entry.exclusion_reason.clone(),
        chunk_refs: None,
        input_ordinal,
        occurrence_index: 0,
    })
}

/// Materialise the entry's bytes and hash them. Bytes always end up in
/// memory because `CasStore::put` takes `&[u8]`; the Path branch reads
/// the file once (fully) and reuses the buffer for both hashing and CAS
/// put. Memory usage per active worker is therefore O(largest entry
/// size). For v1 synthetic test corpora this is small; for the 1 GB
/// acceptance corpus the limit is `num_cpus * largest_doc` which the
/// 100 MiB doc-size upper bound from BUILD-PLAN §1.3 keeps bounded.
fn read_and_hash(entry: &CorpusEntry) -> Result<(StreamHash, Vec<u8>), BuildError> {
    let bytes = match &entry.content {
        ContentSource::Path(path) => fs::read(path).map_err(|source| BuildError::Io {
            source_uri: entry.source_uri.clone(),
            source,
        })?,
        ContentSource::Bytes(b) => b.clone(),
    };
    let hash = stream_hash(&bytes[..]).map_err(|source| BuildError::Io {
        source_uri: entry.source_uri.clone(),
        source,
    })?;
    Ok((hash, bytes))
}
