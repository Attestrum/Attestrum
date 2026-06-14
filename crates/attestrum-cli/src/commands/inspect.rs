//! `attestrum inspect <manifest>` — read a sealed manifest and print a
//! human summary (Merkle root, leaf count, total bytes, per-modality
//! histogram). Pure offline; no network; no mutation.
//!
//! Drives the [`crate::lifecycle`] state machine literally so the
//! shipped behaviour matches `docs/diagrams/sprint-3/attestrum-inspect-lifecycle.md`
//! one-to-one. The lifecycle is pure code (no I/O); this module is the
//! single concrete consumer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use attestrum_core::Modality;
use attestrum_manifest::{read_manifest_metadata, ManifestBatchReader, SCHEMA_VERSION};
use attestrum_merkle::merkle_root;

use crate::lifecycle::{transition, ExitCode, InspectEvent, InspectState};

/// Subcommand arguments. Owned by `main` and passed in by value.
#[derive(Debug)]
pub struct Args {
    pub manifest: PathBuf,
    /// CLI-uniformity flag — `inspect` is always offline, so this is a
    /// no-op acknowledged via tracing.
    pub offline: bool,
}

/// `attestrum inspect` entry point. Returns the numeric process exit code
/// directly; `main` wraps it in `ExitCode::from(...)`. Errors are
/// printed to stderr inside this function (since the exit code is the
/// sole channel back to `main` for this subcommand).
pub fn run(args: Args) -> u8 {
    if args.offline {
        tracing::info!("--offline acknowledged (inspect is always offline)");
    }

    // Lifecycle: we enter at Invoked. clap has already succeeded by the
    // time `run` is called (clap-native parse failures exit before main
    // dispatches), so the first transition is always ClapParseOk.
    let mut state = InspectState::Invoked;
    state = transition(state, InspectEvent::ClapParseOk);

    // ArgsParsed → Validated | Exit(ArgsError)
    if args.manifest.is_file() {
        state = transition(state, InspectEvent::PathExistsAndIsFile);
    } else {
        eprintln!(
            "attestrum inspect: manifest path missing or not a file: {}",
            args.manifest.display()
        );
        state = transition(state, InspectEvent::PathMissingOrNotFile);
        return terminal_code(state);
    }

    // Validated → LocalRead (inspect is always offline → unconditional)
    state = transition(state, InspectEvent::DispatchInspect);

    // LocalRead → ManifestLoaded | Exit(RuntimeError) | Exit(SchemaError)
    //
    // Distinguishing the two error paths:
    //   - Schema mismatch: the file IS a valid Parquet (KeyValue
    //     metadata reads back successfully) but the
    //     `attestrum.manifest.schema_version` value is not
    //     `attestrum_manifest::SCHEMA_VERSION`. This is the explicit
    //     "wrong-version" signal — the versioning slot for future
    //     migrations.
    //   - Runtime error: anything else — file isn't valid Parquet,
    //     file became unreadable mid-read, etc. Mapped to Exit1.
    //
    // Read metadata first because it carries the schema-version
    // assertion. If metadata reads cleanly but the version is wrong,
    // we Exit8 without ever loading the rows.
    // Schema-version gate. read_manifest_metadata reads only the Parquet
    // footer (no row decode), so its failure is the ReadIoError path.
    match read_manifest_metadata(&args.manifest) {
        Ok((schema_version, _writer_profile)) if schema_version != SCHEMA_VERSION => {
            eprintln!(
                "attestrum inspect: schema version mismatch: expected {SCHEMA_VERSION}, got {schema_version}"
            );
            state = transition(state, InspectEvent::ReadSchemaMismatch);
            return terminal_code(state);
        }
        Ok(_) => {}
        Err(e) => {
            // KeyValue metadata read failed → not a recognisable Attestrum
            // Parquet file at all → runtime error.
            eprintln!("attestrum inspect: parquet read failed: {e}");
            state = transition(state, InspectEvent::ReadIoError);
            return terminal_code(state);
        }
    }

    // Stream the manifest in CONSTANT memory to build the summary — never
    // load the whole Vec<ManifestEntry> (~30 GB at 100M rows, OOMs a 16 GB
    // box). Mirrors the streaming `sign` / `publish` / `merge` paths.
    let summary = match summarise(&args.manifest) {
        Ok(s) => {
            state = transition(state, InspectEvent::ReadOk);
            s
        }
        Err(e) => {
            // Metadata succeeded but a row batch failed to decode: treat as
            // schema-mismatch (the file claims schema "1" but doesn't have
            // the column shape we expect).
            eprintln!("attestrum inspect: manifest schema mismatch: {e}");
            state = transition(state, InspectEvent::ReadSchemaMismatch);
            return terminal_code(state);
        }
    };

    // ManifestLoaded → Summarized.
    state = transition(state, InspectEvent::ComputeSummary);

    // Summarized → Exit(Ok): print and terminate.
    print_summary(&summary);
    state = transition(state, InspectEvent::PrintSummary);
    terminal_code(state)
}

fn terminal_code(state: InspectState) -> u8 {
    match state {
        InspectState::Exit(code) => code.as_u8(),
        // The lifecycle should always be terminal by the time `run`
        // returns. Defensive: if a refactor breaks that invariant,
        // surface as a generic runtime error rather than a panic.
        _ => ExitCode::RuntimeError.as_u8(),
    }
}

/// In-memory summary the printer formats. Carrying this through a
/// struct rather than printing inline makes the `manifest_with_zero_entries_*`
/// test's assertions easier to reason about.
#[derive(Debug)]
struct Summary {
    merkle_root: [u8; 32],
    leaf_count: usize,
    total_bytes: u64,
    by_modality: BTreeMap<&'static str, usize>,
}

/// Stream the manifest through the constant-memory [`ManifestBatchReader`],
/// accumulating only the document_id leaf vector (for the root), the byte
/// total, and the per-modality histogram — never the whole `Vec<ManifestEntry>`.
/// Output is identical to the prior full-slice computation (the leaves are
/// collected in the same on-disk order; byte/modality sums are
/// order-independent). Errors surface as strings (the caller maps them to the
/// `ReadSchemaMismatch` transition).
fn summarise(path: &Path) -> Result<Summary, String> {
    let reader = ManifestBatchReader::open(path).map_err(|e| e.to_string())?;
    let mut leaves: Vec<[u8; 32]> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut by_modality: BTreeMap<&'static str, usize> = BTreeMap::new();
    for batch in reader {
        for row in batch.map_err(|e| e.to_string())? {
            leaves.push(row.document_id);
            total_bytes += row.size_bytes;
            *by_modality.entry(modality_label(row.modality)).or_insert(0) += 1;
        }
    }
    let leaf_count = leaves.len();
    let merkle = merkle_root(&leaves);
    Ok(Summary {
        merkle_root: merkle,
        leaf_count,
        total_bytes,
        by_modality,
    })
}

fn modality_label(m: Modality) -> &'static str {
    match m {
        Modality::Text => "text",
        Modality::Image => "image",
        Modality::Audio => "audio",
        Modality::Video => "video",
        Modality::Pdf => "pdf",
        Modality::Other => "other",
    }
}

fn print_summary(s: &Summary) {
    println!("merkle_root: {}", hex_64(&s.merkle_root));
    println!("leaf_count:  {}", s.leaf_count);
    println!("total_bytes: {}", s.total_bytes);
    if s.by_modality.is_empty() {
        println!("per modality: (none)");
    } else {
        println!("per modality:");
        for (label, count) in &s.by_modality {
            println!("  {label}: {count}");
        }
    }
}

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
