//! Sprint 4 E3.6 local determinism gate for the full Sprint 3 pipeline.
//!
//! The cross-target CI matrix at `.github/workflows/determinism.yml`
//! pairwise-`cmp`s `manifest.parquet` + the Merkle root across four
//! targets (linux-x86_64-glibc, linux-aarch64-glibc, macos-aarch64-
//! darwin, linux-x86_64-musl) on every push and nightly. That matrix
//! catches cross-platform byte drift but only when CI runs.
//!
//! This file catches IN-PROCESS non-determinism on every `cargo test`
//! locally — HashMap iteration leakage, env-dependent state, time-seeded
//! RNG, mutable global state, Rayon work-stealing leakage. It exercises
//! the full pipeline including the CAS write path that
//! `crates/attestrum-cas/tests/determinism_local.rs` (Sprint 2 E9) does NOT
//! cover (the cas-side gate only tests `stream_hash + merkle_root`).
//!
//! In-process determinism is **necessary but not sufficient** for cross-
//! target determinism. Both gates are needed; this one runs first and is
//! cheap, the CI matrix runs second and is comprehensive.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::CasStore;
use attestrum_core::{BuildContext, Modality};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildOutput, ContentSource, CorpusEntry};

/// Per-test counter for scoped temp roots — same pattern as
/// `crates/attestrum-pipeline/tests/build_corpus.rs` so parallel test
/// execution doesn't collide.
static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str, instance: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!(
        "attestrum-pipeline-e3.6-{test_name}-{instance}-{n}"
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

/// xorshift64 — deterministic pseudorandom byte stream. Mirror of
/// `crates/attestrum-cas/src/lib.rs` and `crates/attestrum-pipeline/tests/build_corpus.rs`
/// patterns so test corpora are reproducible across platforms with no
/// external RNG dep.
fn xorshift_fill(buf: &mut [u8], seed: u64) {
    let mut state = seed;
    let mut i = 0;
    while i < buf.len() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        let take = (buf.len() - i).min(8);
        buf[i..i + take].copy_from_slice(&bytes[..take]);
        i += take;
    }
}

/// Build the same 200-entry test corpus from scratch. Each entry has a
/// stable `source_uri`, a stable seeded body, and (for half the corpus)
/// a non-trivial language/license/mime so the manifest exercises more
/// columns than the trivial all-defaults path.
fn build_test_entries() -> Vec<CorpusEntry> {
    (0..200u64)
        .map(|i| {
            // Body length varies 100..=4196 bytes so we exercise the
            // streaming hash path for non-trivial sizes.
            let len = 100 + ((i as usize) * 21) % 4096;
            let mut body = vec![0u8; len];
            xorshift_fill(&mut body, i.wrapping_mul(0x9E3779B97F4A7C15));

            CorpusEntry {
                source_uri: format!("file:///docs/doc-{i:04}.bin"),
                content: ContentSource::Bytes(body),
                modality: if i % 3 == 0 {
                    Modality::Image
                } else {
                    Modality::Text
                },
                mime_type: Some(
                    if i % 3 == 0 {
                        "image/png"
                    } else {
                        "text/plain"
                    }
                    .into(),
                ),
                source_type: None,
                source_dataset_id: if i % 5 == 0 {
                    Some(format!("dataset-{}", i / 5))
                } else {
                    None
                },
                registered_domain: if i % 4 == 0 {
                    Some("example.org".into())
                } else {
                    None
                },
                license_spdx: if i % 2 == 0 {
                    Some("CC-BY-4.0".into())
                } else {
                    None
                },
                language: if i % 2 == 0 { Some("en".into()) } else { None },
                fetched_at: None,
                signals: ManifestSignals::default(),
                included: true,
                exclusion_reason: None,
            }
        })
        .collect()
}

/// Run `build_corpus` end-to-end against a fresh CAS + output dir.
/// Returns the BuildOutput + the bytes of the produced manifest.parquet.
fn run_one(test_name: &str, instance: &str) -> (BuildOutput, Vec<u8>) {
    let root = fresh_root(test_name, instance);
    let ctx = BuildContext::new(root.clone(), 0);
    let cas = CasStore::new(root.join(".attestrum")).expect("CasStore::new");
    let out = root.join("out");
    let entries = build_test_entries();
    let output = build_corpus(&ctx, &cas, &entries, &out).expect("build_corpus");
    let manifest_bytes = fs::read(&output.manifest_path).expect("read manifest.parquet");
    (output, manifest_bytes)
}

#[test]
fn sprint_3_pipeline_is_in_process_deterministic() {
    let (first_output, first_bytes) = run_one("pipeline_det", "first");
    let (second_output, second_bytes) = run_one("pipeline_det", "second");

    assert_eq!(
        first_output.merkle_root, second_output.merkle_root,
        "Merkle root diverged across two in-process runs — likely Rayon work-stealing leakage, \
         HashMap iteration order, or unsorted intermediate state"
    );
    assert_eq!(
        first_output.leaf_count, second_output.leaf_count,
        "leaf count diverged across runs"
    );
    assert_eq!(
        first_output.total_bytes, second_output.total_bytes,
        "total bytes diverged across runs"
    );
    assert_eq!(
        first_bytes.len(),
        second_bytes.len(),
        "manifest.parquet length diverged: first={} second={}",
        first_bytes.len(),
        second_bytes.len()
    );
    assert_eq!(
        first_bytes, second_bytes,
        "manifest.parquet bytes diverged across runs — the 4-target CI determinism matrix \
         would catch this on push, but every developer should catch it locally first"
    );
}

#[test]
fn sprint_3_pipeline_output_shape_matches_ci_compare_expectations() {
    let (output, bytes) = run_one("pipeline_shape", "single");
    assert_eq!(output.leaf_count, 200, "expected 200 leaves");
    assert!(
        bytes.len() > 1000,
        "expected non-trivial manifest.parquet size, got {} bytes",
        bytes.len()
    );
    // Parquet files start with the 4-byte magic "PAR1".
    assert_eq!(
        &bytes[..4],
        b"PAR1",
        "expected Parquet PAR1 magic prefix; got {:?}",
        &bytes[..4]
    );
    // And end with the same trailing magic.
    assert_eq!(
        &bytes[bytes.len() - 4..],
        b"PAR1",
        "expected Parquet PAR1 magic suffix"
    );
}
