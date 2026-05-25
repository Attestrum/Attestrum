//! Sprint 3 E8: deterministic manifest-byte-identity assertion across
//! CI targets.
//!
//! Synthesizes a 1000-document deterministic corpus entirely
//! in-memory (no input file I/O, no clocks, no env vars, no RNG),
//! runs the full [`attestrum_pipeline::build_corpus`] pipeline (hash +
//! CAS-put + manifest write + Merkle root), copies the resulting
//! `manifest.parquet` to a caller-specified path, and prints the
//! Merkle root as 64 lowercase hex chars + a single `\n` (exactly
//! 65 bytes) to stdout.
//!
//! Run by every target in `.github/workflows/determinism.yml`. A
//! separate `compare` job in the same workflow downloads all four
//! per-target captures and `cmp`s:
//!
//!   - the `manifest.parquet` bytes pairwise (Sprint 3 E8 addition);
//!   - the pipeline merkle-root hex pairwise (cross-check against the
//!     Sprint 2 E9 sprint-2-corpus example, since both compute the
//!     same Merkle root over the same digest list).
//!
//! The local-only mirror lives at
//! `crates/attestrum-pipeline/tests/cross_platform_inputs.rs`.
//!
//! **Same corpus content as `sprint-2-corpus`** — uses the
//! `annex-sprint-2-doc-{NNNN}` string template so the canonical
//! Sprint 2 root `47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73`
//! reproduces verbatim from this binary too. If you change the corpus
//! shape, update Sprint 2's example + local test + the canonical root
//! value in this comment, all in the same commit.
//!
//! **If you change this file, update
//! `crates/attestrum-pipeline/tests/cross_platform_inputs.rs` in the same
//! commit** — the two share the corpus shape, hash algorithm, sort
//! order, and output format by convention. Drift between them means
//! the local gate stops mirroring what CI will see.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use attestrum_cas::CasStore;
use attestrum_core::{hex, BuildContext, Modality};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, ContentSource, CorpusEntry};

const CORPUS_SIZE: u32 = 1000;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let manifest_target = match args.get(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!(
                "usage: cargo run -p attestrum-pipeline --example sprint-3-corpus -- <manifest-out-path>"
            );
            process::exit(2);
        }
    };

    // Per-process scratch workspace under the system temp dir. The
    // CAS lands at <ws>/.attestrum/cas/; the manifest at
    // <ws>/.attestrum/manifests/manifest.parquet. We copy the manifest to
    // `manifest_target` and best-effort remove the scratch dir.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let workspace = env::temp_dir().join(format!(
        "attestrum-sprint-3-corpus-{}-{nanos}",
        process::id()
    ));
    fs::create_dir_all(&workspace)?;

    // Build the canonical 1000-doc Sprint 3 corpus in-memory. The
    // document body template matches `sprint-2-corpus` (so both
    // pipelines emit the same Merkle root over the same digest list);
    // per-entry metadata is the minimal valid shape.
    //
    // The local test at `tests/cross_platform_inputs.rs` duplicates
    // this synthesis block verbatim — they must stay in sync.
    let entries: Vec<CorpusEntry> = (0..CORPUS_SIZE)
        .map(|i| {
            let body = format!("annex-sprint-2-doc-{i:04}").into_bytes();
            CorpusEntry {
                source_uri: format!("synthetic://sprint-3-doc-{i:04}"),
                content: ContentSource::Bytes(body),
                modality: Modality::Text,
                mime_type: None,
                source_type: None,
                source_dataset_id: None,
                registered_domain: None,
                license_spdx: None,
                language: None,
                fetched_at: None,
                signals: ManifestSignals::default(),
                included: true,
                exclusion_reason: None,
            }
        })
        .collect();
    let cas = CasStore::new(workspace.join(".attestrum")).expect("CasStore::new");
    let ctx = BuildContext::new(workspace.clone(), 0);
    let manifest_dir = workspace.join(".attestrum").join("manifests");
    let output = build_corpus(&ctx, &cas, &entries, &manifest_dir).expect("build_corpus");

    // Ensure the parent dir for the requested manifest path exists.
    if let Some(parent) = manifest_target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::copy(&output.manifest_path, &manifest_target)?;

    // 65-byte stdout: 64 hex chars + `\n`. Matches `sprint-2-corpus`
    // exactly so the CI compare job can cross-validate the two
    // pipelines produce the same Merkle root.
    let hex_root = hex::encode_32(&output.merkle_root);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(hex_root.as_bytes())?;
    out.write_all(b"\n")?;

    // Best-effort scratch cleanup. Failing to clean up is non-fatal —
    // the temp dir will be reaped by the OS or the CI runner.
    let _ = fs::remove_dir_all(&workspace);

    Ok(())
}
