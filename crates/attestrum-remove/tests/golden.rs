//! Determinism + golden + validation coverage for the removal report.
//!
//! The two `prove()` calls embed `file://<abs-path>` manifest URIs, so the raw
//! report is machine-specific by construction. The golden therefore normalizes
//! the two temp-manifest paths to fixed tokens before comparing — exercising the
//! full embedded in-toto Statements while staying machine-independent.
//! Regenerate with
//! `ATTESTRUM_REGEN_REMOVE_GOLDEN=1 cargo test -p attestrum-remove --test golden`.

use attestrum_core::Modality;
use attestrum_manifest::{write_manifest, ManifestEntry, ManifestSignals};
use attestrum_remove::evidence::{build_removal, RemoveError};
use std::path::{Path, PathBuf};

/// Fixed reproducible-build epoch so the proofs' `built_at` is stable.
const EPOCH: i64 = 1_718_200_000;

fn entry(seed: u8) -> ManifestEntry {
    ManifestEntry {
        document_id: [seed; 32],
        sha256: [seed.wrapping_add(1); 32],
        size_bytes: 100 + seed as u64,
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
        input_ordinal: seed as u64,
        occurrence_index: 0,
    }
}

/// Write a manifest with the given document-id seeds (kept ascending so the
/// non-inclusion adjacency proof sees canonical order).
fn write_fixture(path: &Path, seeds: &[u8]) {
    let entries: Vec<ManifestEntry> = seeds.iter().map(|&s| entry(s)).collect();
    write_manifest(path, &entries).expect("write manifest fixture");
}

// A fresh directory per call: tests run in parallel, and several write
// identically-named manifest fixtures, so a shared directory races (one test
// reads a manifest mid-write from another). Path normalization makes the report
// output directory-independent, so unique dirs change nothing about the bytes.
fn fixtures_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "attestrum-remove-golden-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).expect("create temp dir");
    d
}

/// Build the canonical removal report and normalize the two machine-specific
/// manifest paths to fixed tokens.
fn normalized_report_json() -> String {
    let dir = fixtures_dir();
    let before = dir.join("before.parquet");
    let after = dir.join("after.parquet");
    // target [20; 32] is present in `before`, removed from `after`, and sits
    // between [10; 32] and [30; 32] → Interior non-inclusion adjacency.
    write_fixture(&before, &[10, 20, 30]);
    write_fixture(&after, &[10, 30]);

    let report = build_removal([20u8; 32], &before, &after, EPOCH).expect("build removal");
    let json = report.to_json().expect("serialize report");
    json.replace(&before.display().to_string(), "<BEFORE_MANIFEST>")
        .replace(&after.display().to_string(), "<AFTER_MANIFEST>")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("report.json")
}

#[test]
fn proofs_have_expected_kinds() {
    let dir = fixtures_dir();
    let before = dir.join("kinds-before.parquet");
    let after = dir.join("kinds-after.parquet");
    write_fixture(&before, &[10, 20, 30]);
    write_fixture(&after, &[10, 30]);

    let report = build_removal([20u8; 32], &before, &after, EPOCH).expect("build removal");
    assert!(report.removed);
    assert_eq!(report.before.proof_kind, "inclusion");
    assert_eq!(report.after.proof_kind, "non-inclusion");
    assert_eq!(report.target, "14".repeat(32)); // 0x14 == 20
}

#[test]
fn errors_when_target_absent_from_before() {
    let dir = fixtures_dir();
    let before = dir.join("absent-before.parquet");
    let after = dir.join("absent-after.parquet");
    write_fixture(&before, &[10, 30]); // no 20
    write_fixture(&after, &[10, 30]);

    let err = build_removal([20u8; 32], &before, &after, EPOCH).expect_err("must reject");
    assert!(
        matches!(err, RemoveError::TargetNotInBefore(_)),
        "got {err:?}"
    );
}

#[test]
fn errors_when_target_still_present_in_after() {
    let dir = fixtures_dir();
    let before = dir.join("still-before.parquet");
    let after = dir.join("still-after.parquet");
    write_fixture(&before, &[10, 20, 30]);
    write_fixture(&after, &[10, 20, 30]); // 20 still there

    let err = build_removal([20u8; 32], &before, &after, EPOCH).expect_err("must reject");
    assert!(
        matches!(err, RemoveError::TargetStillInAfter(_)),
        "got {err:?}"
    );
}

#[test]
fn report_is_in_process_deterministic() {
    assert_eq!(
        normalized_report_json(),
        normalized_report_json(),
        "normalized report.json bytes diverged across two in-process runs"
    );
}

#[test]
fn report_matches_committed_golden() {
    let derived = normalized_report_json();
    let path = golden_path();
    if std::env::var("ATTESTRUM_REGEN_REMOVE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, &derived).expect("regen write golden");
        eprintln!("regenerated {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("read golden report.json");
    assert_eq!(
        derived,
        expected,
        "report.json differs from committed golden {}; if intended, regenerate with \
         ATTESTRUM_REGEN_REMOVE_GOLDEN=1",
        path.display()
    );
}
