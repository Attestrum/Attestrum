//! `attestrum merge --inputs <files...> --out <merged.parquet>` — merges
//! N already-sealed shard manifests into one. Reads each shard
//! manifest via `attestrum_manifest::read_manifest`, concatenates rows in
//! lexicographic input-path order (deterministic), re-runs
//! `assign_occurrence_indices` GLOBALLY so cross-shard identical
//! digests get a unified occurrence count, then re-runs `sort_entries`
//! and writes the merged manifest.
//!
//! See `docs/diagrams/sprint-3/sharding.md` for the determinism
//! contract: the merged Merkle root ALWAYS equals the root of an
//! unsharded build of the same logical corpus (multiset Merkle is
//! invariant under within-group permutation). The merged
//! `manifest.parquet` BYTES additionally equal the unsharded variant
//! when no cross-shard duplicate digests exist (or when they happen to
//! align — the typical case for production corpora where duplicate
//! content shares a source_url and therefore co-locates to the same
//! shard per `attestrum plan`'s assignment rule).

use std::fs;
use std::path::PathBuf;

use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, read_manifest, sort_entries, write_manifest,
    ManifestEntry,
};
use thiserror::Error;

/// Subcommand arguments.
#[derive(Debug)]
pub struct Args {
    pub inputs: Vec<PathBuf>,
    pub out: PathBuf,
}

/// Errors `merge::run` can surface. All map to exit code 1.
#[derive(Debug, Error)]
pub enum MergeError {
    #[error("--inputs must be non-empty; pass at least one shard manifest")]
    NoInputs,

    #[error("input manifest read failed at {path}: {source}")]
    InputRead {
        path: PathBuf,
        #[source]
        source: attestrum_core::AttestrumError,
    },

    #[error("output dir prepare failed at {path}: {source}")]
    OutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("merged manifest write failed: {0}")]
    Write(#[from] attestrum_core::AttestrumError),
}

/// `attestrum merge` entry point. Returns 0 on success, 1 on any error.
/// All errors are printed to stderr inside this function.
pub fn run(args: Args) -> u8 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("attestrum merge: {err}");
            let mut source = std::error::Error::source(&err);
            while let Some(s) = source {
                eprintln!("  caused by: {s}");
                source = std::error::Error::source(s);
            }
            1
        }
    }
}

fn run_inner(args: Args) -> Result<(), MergeError> {
    if args.inputs.is_empty() {
        return Err(MergeError::NoInputs);
    }

    // Sort input paths lexicographically. The user's shell may have
    // already done this (glob expansion), but be defensive — the
    // concatenation order determines the global `assign_occurrence_indices`
    // walk order, which is the only non-trivially-deterministic piece
    // of the merge.
    let mut sorted_inputs = args.inputs.clone();
    sorted_inputs.sort();

    let mut merged: Vec<ManifestEntry> = Vec::new();
    for input in &sorted_inputs {
        let rows = read_manifest(input).map_err(|source| MergeError::InputRead {
            path: input.clone(),
            source,
        })?;
        merged.extend(rows);
    }

    // Global passes (mirror the epilogue from `attestrum_pipeline::build_corpus`
    // adapted to merge semantics):
    //
    //   1. Re-assign `input_ordinal` so each row has a unique 0..N value
    //      in concat order. Each shard manifest carried per-shard ordinals
    //      (rows from two shards could both have ordinal 0), which would
    //      break E2.5's audit invariant in the merged file. Re-stamping
    //      makes the merged manifest auditable on its own terms — the
    //      ordering is the merge concat order rather than the original
    //      pre-shard input order, but the invariant "within each digest
    //      group, sort-by-input_ordinal rank equals occurrence_index"
    //      holds end-to-end.
    //
    //   2. Re-assign `occurrence_index` globally so cross-shard
    //      identical digests get a unified counter (a digest that
    //      appeared as occurrence 0 in two different shards becomes
    //      occurrence 0 and 1 in the merged manifest).
    //
    //   3. Canonical sort by `(document_id, occurrence_index)`.
    //
    // The merged Merkle root computed over the sorted digests equals
    // the root of an unsharded build (multiset Merkle is invariant
    // under within-group permutation). The merged `manifest.parquet`
    // BYTES, however, do NOT generally match unsharded — `input_ordinal`
    // reflects merge concat order rather than original input order.
    // Documented in `docs/diagrams/sprint-3/sharding.md` body.
    assign_input_ordinals(&mut merged);
    assign_occurrence_indices(&mut merged);
    sort_entries(&mut merged);

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| MergeError::OutputDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    write_manifest(&args.out, &merged)?;

    tracing::info!(
        inputs = sorted_inputs.len(),
        out = %args.out.display(),
        rows = merged.len(),
        "merge complete"
    );
    println!("attestrum merge: ok");
    println!("  inputs: {}", sorted_inputs.len());
    println!("  rows:   {}", merged.len());
    println!("  out:    {}", args.out.display());
    Ok(())
}
