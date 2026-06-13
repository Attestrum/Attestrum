//! Determinism + golden coverage for the near-duplicate report.
//!
//! Two guarantees, mirroring `attestrum-decontaminate`'s determinism suite:
//!   1. **Double-run byte-identity** — the same corpus serializes to identical
//!      `report.json` bytes within a process.
//!   2. **Committed golden** — the bytes match a checked-in golden, so the
//!      cross-target CI matrix surfaces any platform divergence.
//!
//! The fixture exercises both clustering paths: an exact-duplicate pair (the
//! signature-collapse path) and a near-duplicate pair (the LSH-banding + Jaccard
//! verify path). Built in-process with fixed corpus labels, so the golden is
//! machine-independent. Regenerate with
//! `ATTESTRUM_REGEN_DEDUP_GOLDEN=1 cargo test -p attestrum-dedup --test golden`.

use attestrum_decontaminate::ingest::Doc;
use attestrum_dedup::cluster::dedup;
use attestrum_dedup::report;
use std::path::PathBuf;

/// `n` distinct words starting at index `start`: "w{start} w{start+1} ...".
fn words(start: usize, n: usize) -> String {
    (start..start + n)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn doc(id: &str, text: String) -> Doc {
    Doc {
        id: id.to_string(),
        text,
    }
}

fn fixture_docs() -> Vec<Doc> {
    // Near pair: 120-word base, and a copy with three mid-document words changed
    // (high Jaccard, but NOT identical → must cluster via LSH + verify).
    let base: Vec<String> = (0..120).map(|i| format!("w{i}")).collect();
    let mut near = base.clone();
    near[58] = "x".to_string();
    near[59] = "y".to_string();
    near[60] = "z".to_string();

    vec![
        doc("near-a", base.join(" ")),
        doc("near-b", near.join(" ")),
        // Exact-duplicate pair (signature-collapse path).
        doc("exact-a", words(500, 40)),
        doc("exact-b", words(500, 40)),
        // Two uniques.
        doc("unique-a", words(1000, 30)),
        doc("unique-b", words(2000, 30)),
    ]
}

fn fixture_report_json() -> String {
    let docs = fixture_docs();
    let result = dedup(&docs, 0.8);
    let report = report::build(vec!["corpus.jsonl".to_string()], &result, 0.8, None);
    report.to_json().expect("serialize report")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("report.json")
}

#[test]
fn fixture_exercises_both_clustering_paths() {
    let docs = fixture_docs();
    let result = dedup(&docs, 0.8);
    // Exactly two clusters: the exact pair and the near pair.
    assert_eq!(result.clusters.len(), 2, "expected near + exact clusters");
    assert_eq!(result.near_duplicate_documents, 4);
}

#[test]
fn report_is_in_process_deterministic() {
    assert_eq!(
        fixture_report_json(),
        fixture_report_json(),
        "report.json bytes diverged across two in-process runs"
    );
}

#[test]
fn report_matches_committed_golden() {
    let derived = fixture_report_json();
    let path = golden_path();
    if std::env::var("ATTESTRUM_REGEN_DEDUP_GOLDEN").is_ok() {
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
         ATTESTRUM_REGEN_DEDUP_GOLDEN=1",
        path.display()
    );
}
