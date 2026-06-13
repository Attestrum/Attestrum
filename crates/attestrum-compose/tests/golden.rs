//! Determinism + golden coverage for the composition report.
//!
//! Three guarantees, mirroring `attestrum-decontaminate`'s determinism suite:
//!   1. **Double-run byte-identity** — the same entries serialize to identical
//!      `report.json` bytes within a process.
//!   2. **Committed golden** — the bytes match a checked-in golden, so the
//!      cross-target CI matrix surfaces any platform divergence.
//!   3. **File path == memory path** — `aggregate_manifest` (Parquet reader)
//!      agrees with `aggregate_entries` (in-memory) on the same entries.
//!
//! The fixture is built in-process with a fixed manifest label (no filesystem
//! path), so the golden is machine-independent. Regenerate with
//! `ATTESTRUM_REGEN_COMPOSE_GOLDEN=1 cargo test -p attestrum-compose --test golden`.

use attestrum_compose::aggregate::{aggregate_entries, aggregate_manifest};
use attestrum_compose::report;
use attestrum_core::{Modality, SourceType};
use attestrum_manifest::{write_manifest, ManifestEntry, ManifestSignals};
use std::path::PathBuf;

const MANIFEST_LABEL: &str = "fixture-manifest.parquet";

#[allow(clippy::too_many_arguments)]
fn entry(
    seed: u8,
    modality: Modality,
    source_type: Option<SourceType>,
    license: Option<&str>,
    language: Option<&str>,
    size_bytes: u64,
    included: bool,
) -> ManifestEntry {
    ManifestEntry {
        document_id: [seed; 32],
        sha256: [seed.wrapping_add(1); 32],
        size_bytes,
        modality,
        mime_type: None,
        source_url: None,
        source_type,
        source_dataset_id: None,
        registered_domain: None,
        license_spdx: license.map(str::to_string),
        language: language.map(str::to_string),
        fetched_at: None,
        signals: ManifestSignals::default(),
        included,
        exclusion_reason: if included {
            None
        } else {
            Some("robots_disallow".to_string())
        },
        chunk_refs: None,
        input_ordinal: seed as u64,
        occurrence_index: 0,
    }
}

/// Canonical fixture: ascending `document_id`s (so written order == canonical
/// order), exercising known values, an unspecified source/license/language, and
/// one excluded row.
fn fixture_entries() -> Vec<ManifestEntry> {
    vec![
        entry(
            1,
            Modality::Text,
            Some(SourceType::Crawl),
            Some("CC-BY-4.0"),
            Some("en"),
            100,
            true,
        ),
        entry(
            2,
            Modality::Text,
            Some(SourceType::Crawl),
            None,
            Some("en"),
            200,
            true,
        ),
        entry(
            3,
            Modality::Image,
            Some(SourceType::PublicDataset),
            Some("CC0-1.0"),
            None,
            300,
            true,
        ),
        entry(4, Modality::Text, None, None, Some("fr"), 150, true),
        entry(
            5,
            Modality::Pdf,
            Some(SourceType::PrivateLicensed),
            Some("MIT"),
            Some("de"),
            500,
            false,
        ),
    ]
}

fn fixture_report_json() -> String {
    let entries = fixture_entries();
    let comp = aggregate_entries(entries.iter());
    let report = report::build(MANIFEST_LABEL.to_string(), &comp, None);
    report.to_json().expect("serialize report")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("report.json")
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
    if std::env::var("ATTESTRUM_REGEN_COMPOSE_GOLDEN").is_ok() {
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
         ATTESTRUM_REGEN_COMPOSE_GOLDEN=1",
        path.display()
    );
}

#[test]
fn file_backed_path_matches_in_memory() {
    let entries = fixture_entries();
    let dir = std::env::temp_dir().join("attestrum-compose-golden-test");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("manifest.parquet");
    write_manifest(&path, &entries).expect("write manifest fixture");

    let from_file = aggregate_manifest(&path).expect("aggregate manifest");
    let from_mem = aggregate_entries(entries.iter());

    assert_eq!(from_file.total_documents, from_mem.total_documents);
    assert_eq!(from_file.included_documents, from_mem.included_documents);
    assert_eq!(from_file.excluded_documents, from_mem.excluded_documents);
    assert_eq!(
        from_file.merkle_root, from_mem.merkle_root,
        "Parquet-read root must equal in-memory root"
    );
    assert_eq!(
        from_file.language.buckets.len(),
        from_mem.language.buckets.len()
    );
}
