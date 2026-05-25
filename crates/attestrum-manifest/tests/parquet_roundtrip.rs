//! Integration tests for `attestrum_manifest::io::{write_manifest, read_manifest}`.
//!
//! Covers the test obligations per CLAUDE.md §7.1 (erDiagram → schema
//! roundtrip) named in `docs/diagrams/sprint-3/manifest-schema.md`:
//!
//! - `parquet_write_then_read_returns_equal_entries`
//! - `parquet_byte_identical_re_write` (in-process determinism)
//! - `multiset_three_copies_get_occurrence_indices_012`
//! - `nullable_fields_roundtrip_as_none_when_absent`
//! - `audit_invariant_holds_post_sort` (in lib.rs tests, integration-style here)
//! - `schema_version_keyvalue_metadata_pinned_to_1`
//!
//! Plus the cross-check's flagged risks:
//! - `all_null_chunk_refs_roundtrip_bytes_stable` (R2's medium-confidence
//!   concern about all-NULL List<FixedSizeBinary(32)> definition-level bytes)

use std::path::PathBuf;

use attestrum_core::{Modality, SourceType};
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, read_manifest, read_manifest_metadata,
    sort_entries, write_manifest, ManifestEntry, ManifestSignals, SCHEMA_VERSION, WRITER_PROFILE,
};

fn digest(b: u8) -> [u8; 32] {
    [b; 32]
}

fn sample_entry(doc_byte: u8) -> ManifestEntry {
    ManifestEntry {
        document_id: digest(doc_byte),
        sha256: digest(doc_byte ^ 0xff),
        size_bytes: u64::from(doc_byte) * 100,
        modality: Modality::Text,
        mime_type: Some("text/plain".into()),
        source_url: Some(format!("file:///docs/doc-{doc_byte:02x}.txt")),
        source_type: Some(SourceType::PublicDataset),
        source_dataset_id: Some("common-pile-mini".into()),
        registered_domain: None,
        license_spdx: Some("CC0-1.0".into()),
        language: Some("en".into()),
        fetched_at: Some(1_700_000_000),
        signals: ManifestSignals::default(),
        included: true,
        exclusion_reason: None,
        chunk_refs: None,
        input_ordinal: 0,
        occurrence_index: 0,
    }
}

fn tmp_path(name: &str) -> PathBuf {
    let dir = std::env::var("CARGO_TARGET_TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    PathBuf::from(dir).join(format!("attestrum-manifest-{name}-{pid}-{nanos}.parquet"))
}

#[test]
fn parquet_write_then_read_returns_equal_entries() {
    let mut entries: Vec<ManifestEntry> = (0u8..16).map(sample_entry).collect();
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);

    let path = tmp_path("rt-equal");
    write_manifest(&path, &entries).expect("write ok");
    let read_back = read_manifest(&path).expect("read ok");
    let _ = std::fs::remove_file(&path);

    assert_eq!(read_back.len(), entries.len());
    assert_eq!(read_back, entries);
}

#[test]
fn parquet_byte_identical_re_write() {
    let mut entries: Vec<ManifestEntry> = (0u8..32).map(sample_entry).collect();
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);

    let a = tmp_path("det-a");
    let b = tmp_path("det-b");
    write_manifest(&a, &entries).expect("write a");
    write_manifest(&b, &entries).expect("write b");
    let bytes_a = std::fs::read(&a).expect("read a");
    let bytes_b = std::fs::read(&b).expect("read b");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);

    assert_eq!(
        bytes_a.len(),
        bytes_b.len(),
        "byte-length mismatch: a={} b={}",
        bytes_a.len(),
        bytes_b.len()
    );
    assert!(
        bytes_a == bytes_b,
        "two writes of the same input produced different bytes (determinism failure)"
    );
}

#[test]
fn multiset_three_copies_get_occurrence_indices_012() {
    // Three input entries with identical document_id (and identical content
    // since sample_entry is deterministic from doc_byte) should land in the
    // Parquet file with input_ordinal {0,1,2} and occurrence_index {0,1,2}.
    let mut entries = vec![sample_entry(0xaa), sample_entry(0xaa), sample_entry(0xaa)];
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);

    let path = tmp_path("multiset-3");
    write_manifest(&path, &entries).expect("write ok");
    let read_back = read_manifest(&path).expect("read ok");
    let _ = std::fs::remove_file(&path);

    assert_eq!(read_back.len(), 3);
    let occs: Vec<u32> = read_back.iter().map(|e| e.occurrence_index).collect();
    let ins: Vec<u64> = read_back.iter().map(|e| e.input_ordinal).collect();
    assert_eq!(occs, vec![0, 1, 2]);
    assert_eq!(ins, vec![0, 1, 2]);
    // All three rows must share the same document_id.
    assert_eq!(read_back[0].document_id, read_back[1].document_id);
    assert_eq!(read_back[1].document_id, read_back[2].document_id);
}

#[test]
fn nullable_fields_roundtrip_as_none_when_absent() {
    let entry = ManifestEntry {
        document_id: digest(0x01),
        sha256: digest(0x02),
        size_bytes: 0,
        modality: Modality::Other,
        mime_type: None,
        source_url: None,
        source_type: None,
        source_dataset_id: None,
        registered_domain: None,
        license_spdx: None,
        language: None,
        fetched_at: None,
        signals: ManifestSignals::default(),
        included: false,
        exclusion_reason: Some("nothing expressed a preference".into()),
        chunk_refs: None,
        input_ordinal: 0,
        occurrence_index: 0,
    };
    let entries = vec![entry.clone()];
    let path = tmp_path("nulls");
    write_manifest(&path, &entries).expect("write ok");
    let read_back = read_manifest(&path).expect("read ok");
    let _ = std::fs::remove_file(&path);

    assert_eq!(read_back.len(), 1);
    let got = &read_back[0];
    assert_eq!(got.mime_type, None);
    assert_eq!(got.source_url, None);
    assert_eq!(got.source_type, None);
    assert_eq!(got.source_dataset_id, None);
    assert_eq!(got.registered_domain, None);
    assert_eq!(got.license_spdx, None);
    assert_eq!(got.language, None);
    assert_eq!(got.fetched_at, None);
    assert_eq!(got.chunk_refs, None);
    assert_eq!(
        got.exclusion_reason.as_deref(),
        Some("nothing expressed a preference")
    );
    assert_eq!(got, &entry);
}

#[test]
fn schema_version_keyvalue_metadata_pinned() {
    let entries = vec![sample_entry(0x42)];
    let path = tmp_path("kv");
    write_manifest(&path, &entries).expect("write ok");
    let (sv, wp) = read_manifest_metadata(&path).expect("kv read ok");
    let _ = std::fs::remove_file(&path);

    assert_eq!(sv, SCHEMA_VERSION);
    assert_eq!(wp, WRITER_PROFILE);
}

#[test]
fn all_null_chunk_refs_roundtrip_bytes_stable() {
    // R2's medium-confidence concern from the cross-check: all-NULL
    // List<FixedSizeBinary(32)> columns have historically had varying
    // definition-level bytes depending on builder internal state.
    // Lock this down: build the same 50-entry corpus twice via independent
    // construction paths, write each, assert byte-identical output.
    let mut a: Vec<ManifestEntry> = (0u8..50).map(sample_entry).collect();
    let mut b: Vec<ManifestEntry> = Vec::with_capacity(50);
    for byte in 0u8..50 {
        b.push(sample_entry(byte));
    }
    for entries in [&mut a, &mut b] {
        assign_input_ordinals(entries);
        assign_occurrence_indices(entries);
        sort_entries(entries);
    }
    // All entries have chunk_refs: None — exercises the all-NULL list case.
    for e in a.iter().chain(b.iter()) {
        assert!(e.chunk_refs.is_none());
    }

    let path_a = tmp_path("nullist-a");
    let path_b = tmp_path("nullist-b");
    write_manifest(&path_a, &a).expect("write a");
    write_manifest(&path_b, &b).expect("write b");
    let bytes_a = std::fs::read(&path_a).expect("read a");
    let bytes_b = std::fs::read(&path_b).expect("read b");
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn populated_chunk_refs_roundtrip() {
    let mut entry = sample_entry(0x10);
    entry.chunk_refs = Some(vec![digest(0x11), digest(0x12), digest(0x13)]);
    let entries = vec![entry.clone()];
    let path = tmp_path("chunks");
    write_manifest(&path, &entries).expect("write ok");
    let read_back = read_manifest(&path).expect("read ok");
    let _ = std::fs::remove_file(&path);

    assert_eq!(read_back.len(), 1);
    assert_eq!(read_back[0], entry);
    assert_eq!(read_back[0].chunk_refs.as_ref().unwrap().len(), 3);
}

#[test]
fn empty_manifest_roundtrip() {
    let entries: Vec<ManifestEntry> = Vec::new();
    let path = tmp_path("empty");
    write_manifest(&path, &entries).expect("write empty ok");
    let read_back = read_manifest(&path).expect("read empty ok");
    let _ = std::fs::remove_file(&path);
    assert!(read_back.is_empty());
}

#[test]
fn full_pipeline_canonical_order_roundtrip() {
    // D A B A C B A — same shape as the lib.rs audit-invariant test, but
    // through the Parquet write/read path.
    let mut entries = vec![
        sample_entry(0xdd),
        sample_entry(0xaa),
        sample_entry(0xbb),
        sample_entry(0xaa),
        sample_entry(0xcc),
        sample_entry(0xbb),
        sample_entry(0xaa),
    ];
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);
    let path = tmp_path("canonical");
    write_manifest(&path, &entries).expect("write ok");
    let read_back = read_manifest(&path).expect("read ok");
    let _ = std::fs::remove_file(&path);

    let observed: Vec<(u8, u32)> = read_back
        .iter()
        .map(|e| (e.document_id[0], e.occurrence_index))
        .collect();
    assert_eq!(
        observed,
        vec![
            (0xaa, 0),
            (0xaa, 1),
            (0xaa, 2),
            (0xbb, 0),
            (0xbb, 1),
            (0xcc, 0),
            (0xdd, 0),
        ]
    );
    assert_eq!(read_back, entries);
}
