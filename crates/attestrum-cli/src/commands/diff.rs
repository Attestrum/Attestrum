//! `attestrum diff <OLD> <NEW>` — read two sealed manifests and report the
//! corpus-version delta (added / removed / unchanged documents, multiset shifts,
//! and per-source / per-modality composition shift) as a deterministic, unsigned
//! report. Read-only; never mutates either manifest.
//!
//! Both manifests are streamed via [`ManifestBatchReader`] and joined in
//! `document_id` order by [`attestrum_diff::compare`], so peak memory is the
//! per-side leaf-digest vectors (for each endpoint's Merkle root) plus one batch
//! per reader — never the full manifests. Mirrors `inspect`'s validation: a
//! missing path exits 2, a schema-version mismatch exits 8, any other read
//! failure exits 1.

use std::path::PathBuf;

use attestrum_diff::{compare, render_json, render_summary};
use attestrum_manifest::{
    read_manifest_metadata, ManifestBatchReader, ManifestEntry, SCHEMA_VERSION,
};

use crate::lifecycle::ExitCode;

/// Subcommand arguments. Owned by `main` and passed in by value.
#[derive(Debug)]
pub struct Args {
    pub old: PathBuf,
    pub new: PathBuf,
    /// Optional path for the deterministic `report.json`. The human summary
    /// always prints to stdout regardless.
    pub out: Option<PathBuf>,
    /// Reproducible-Builds timestamp embedded verbatim; `None` → no wall-clock.
    pub timestamp: Option<String>,
}

/// `attestrum diff` entry point. Returns the numeric exit code; `main` wraps it
/// in `ExitCode::from(...)`. Errors print to stderr.
pub fn run(args: Args) -> u8 {
    // Both paths must exist and be files.
    for (label, path) in [("old", &args.old), ("new", &args.new)] {
        if !path.is_file() {
            eprintln!(
                "attestrum diff: {label} manifest path missing or not a file: {}",
                path.display()
            );
            return ExitCode::ArgsError.as_u8();
        }
    }

    // Schema-version check on both, metadata-first (no rows loaded if wrong).
    for (label, path) in [("old", &args.old), ("new", &args.new)] {
        match read_manifest_metadata(path) {
            Ok((schema_version, _)) if schema_version != SCHEMA_VERSION => {
                eprintln!(
                    "attestrum diff: {label} schema version mismatch: expected {SCHEMA_VERSION}, got {schema_version}"
                );
                return ExitCode::SchemaError.as_u8();
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("attestrum diff: {label} parquet read failed: {e}");
                return ExitCode::RuntimeError.as_u8();
            }
        }
    }

    // Open streaming readers and merge-join.
    let old_reader = match ManifestBatchReader::open(&args.old) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("attestrum diff: opening old manifest failed: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };
    let new_reader = match ManifestBatchReader::open(&args.new) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("attestrum diff: opening new manifest failed: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    let report = match compare(entries(old_reader), entries(new_reader), args.timestamp) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("attestrum diff: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    // Optional machine-readable report.
    if let Some(out) = &args.out {
        let json = match render_json(&report) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("attestrum diff: rendering report JSON failed: {e}");
                return ExitCode::RuntimeError.as_u8();
            }
        };
        if let Err(e) = std::fs::write(out, format!("{json}\n")) {
            eprintln!(
                "attestrum diff: writing report to {} failed: {e}",
                out.display()
            );
            return ExitCode::RuntimeError.as_u8();
        }
        eprintln!("attestrum diff: wrote {}", out.display());
    }

    // Human summary always to stdout.
    print!("{}", render_summary(&report));
    ExitCode::Ok.as_u8()
}

/// Flatten a streaming [`ManifestBatchReader`] into per-entry results for
/// [`compare`]. One batch (≤ 8192 rows) is materialized at a time — a read
/// failure surfaces as a single `Err` item that `compare` propagates.
fn entries(
    reader: ManifestBatchReader,
) -> impl Iterator<Item = attestrum_core::Result<ManifestEntry>> {
    reader.flat_map(|batch| match batch {
        Ok(rows) => rows.into_iter().map(Ok).collect::<Vec<_>>().into_iter(),
        Err(e) => vec![Err(e)].into_iter(),
    })
}
