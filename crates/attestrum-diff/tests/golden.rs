//! Golden-file + determinism tests for the corpus-version diff report.
//!
//! The fixture is a small but realistic curation pass over an "old" corpus:
//! drop a short doc, de-duplicate a doubled doc, and add one new doc — exercising
//! every delta category (added / removed / unchanged / multiset shift) and a
//! composition shift across all five lenses.
//!
//! Regenerate the golden after an intentional report-shape change:
//!
//! ```text
//! ATTESTRUM_REGEN_DIFF_GOLDEN=1 cargo test -p attestrum-diff --test golden
//! ```

use std::fs;
use std::path::PathBuf;

use attestrum_core::{Modality, Result, SourceType};
use attestrum_diff::{compare, render_json};
use attestrum_manifest::{sort_entries, ManifestEntry, ManifestSignals};

const GOLDEN: &str = "tests/golden/report.json";

#[allow(clippy::too_many_arguments)]
fn entry(
    id: u8,
    occurrence_index: u32,
    modality: Modality,
    source_type: Option<SourceType>,
    dataset: Option<&str>,
    license: Option<&str>,
    language: Option<&str>,
    size_bytes: u64,
) -> ManifestEntry {
    ManifestEntry {
        document_id: [id; 32],
        sha256: [0u8; 32],
        size_bytes,
        modality,
        mime_type: None,
        source_url: None,
        source_type,
        source_dataset_id: dataset.map(str::to_string),
        registered_domain: None,
        license_spdx: license.map(str::to_string),
        language: language.map(str::to_string),
        fetched_at: None,
        signals: ManifestSignals::default(),
        included: true,
        exclusion_reason: None,
        chunk_refs: None,
        input_ordinal: id as u64,
        occurrence_index,
    }
}

/// Old corpus: 7 rows / 6 distinct (id 2 is doubled).
fn old_corpus() -> Vec<ManifestEntry> {
    let mut v = vec![
        entry(
            1,
            0,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            Some("CC-BY-4.0"),
            Some("en"),
            120,
        ),
        entry(
            2,
            0,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            Some("CC-BY-4.0"),
            Some("en"),
            80,
        ),
        entry(
            2,
            1,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            Some("CC-BY-4.0"),
            Some("en"),
            80,
        ),
        entry(
            3,
            0,
            Modality::Text,
            Some(SourceType::PublicDataset),
            Some("books"),
            Some("MIT"),
            Some("en"),
            200,
        ),
        entry(
            4,
            0,
            Modality::Image,
            Some(SourceType::Crawl),
            Some("web"),
            None,
            None,
            300,
        ),
        entry(
            5,
            0,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            None,
            Some("fr"),
            25,
        ),
    ];
    sort_entries(&mut v);
    v
}

/// New corpus: the curation pass — drop the short doc (id 5), de-duplicate id 2
/// to a single occurrence, add a new doc (id 6).
fn new_corpus() -> Vec<ManifestEntry> {
    let mut v = vec![
        entry(
            1,
            0,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            Some("CC-BY-4.0"),
            Some("en"),
            120,
        ),
        entry(
            2,
            0,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("web"),
            Some("CC-BY-4.0"),
            Some("en"),
            80,
        ),
        entry(
            3,
            0,
            Modality::Text,
            Some(SourceType::PublicDataset),
            Some("books"),
            Some("MIT"),
            Some("en"),
            200,
        ),
        entry(
            4,
            0,
            Modality::Image,
            Some(SourceType::Crawl),
            Some("web"),
            None,
            None,
            300,
        ),
        entry(
            6,
            0,
            Modality::Text,
            Some(SourceType::PublicDataset),
            Some("books"),
            Some("MIT"),
            Some("en"),
            150,
        ),
    ];
    sort_entries(&mut v);
    v
}

fn ok_iter(v: Vec<ManifestEntry>) -> impl Iterator<Item = Result<ManifestEntry>> {
    v.into_iter().map(Ok)
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN)
}

#[test]
fn report_matches_golden() {
    let report = compare(ok_iter(old_corpus()), ok_iter(new_corpus()), None).expect("compare ok");

    // Sanity-check the headline numbers before pinning bytes.
    assert_eq!(
        (
            report.delta.added,
            report.delta.removed,
            report.delta.unchanged
        ),
        (1, 1, 4),
        "added=id6, removed=id5, unchanged={{1,2,3,4}}"
    );
    assert_eq!(report.delta.multiset_shifts.len(), 1, "id 2 went 2x -> 1x");

    let actual = render_json(&report).expect("render json");
    let path = golden_path();

    if std::env::var("ATTESTRUM_REGEN_DIFF_GOLDEN").is_ok() {
        fs::create_dir_all(path.parent().unwrap()).expect("mkdir golden");
        fs::write(&path, format!("{actual}\n")).expect("write golden");
        eprintln!("regenerated golden at {}", path.display());
        return;
    }

    let expected =
        fs::read_to_string(&path).expect("read golden (regen with ATTESTRUM_REGEN_DIFF_GOLDEN=1)");
    assert_eq!(
        actual,
        expected.trim_end_matches('\n'),
        "report.json differs from committed golden"
    );
}

#[test]
fn report_is_byte_deterministic_across_runs() {
    let first =
        render_json(&compare(ok_iter(old_corpus()), ok_iter(new_corpus()), None).unwrap()).unwrap();
    let second =
        render_json(&compare(ok_iter(old_corpus()), ok_iter(new_corpus()), None).unwrap()).unwrap();
    assert_eq!(
        first, second,
        "two compares of the same inputs must render identical bytes"
    );
}

#[test]
fn timestamp_is_embedded_verbatim_when_supplied() {
    let report = compare(
        ok_iter(old_corpus()),
        ok_iter(new_corpus()),
        Some("2026-06-12T00:00:00Z".to_string()),
    )
    .unwrap();
    let json = render_json(&report).unwrap();
    assert!(json.contains("2026-06-12T00:00:00Z"));
    // And absent (no wall-clock) when not supplied.
    let no_ts =
        render_json(&compare(ok_iter(old_corpus()), ok_iter(new_corpus()), None).unwrap()).unwrap();
    assert!(!no_ts.contains("timestamp"));
}
