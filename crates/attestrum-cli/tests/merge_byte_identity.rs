//! Differential byte-identity tests for the streaming `attestrum merge`
//! (`crates/attestrum-cli/src/commands/merge.rs`) — the determinism proof for
//! the streaming k-way merge.
//!
//! Each case builds shard manifests directly via the public `attestrum-manifest`
//! API using exactly the per-shard seal epilogue (`assign_input_ordinals` →
//! `assign_occurrence_indices` → `sort_entries` → `write_manifest`), runs the
//! compiled `attestrum merge`, and asserts the merged `manifest.parquet` is
//! BYTE-IDENTICAL to an in-process reference merge (the previous algorithm:
//! concat all shards in lex-sorted path order, re-run the global passes, write
//! once). Byte-identity is the strongest possible determinism guarantee: it
//! implies identical `document_id` leaves in identical order, hence an identical
//! Merkle root — additionally cross-checked here against an independent
//! `attestrum_merkle::merkle_root` over the reference's leaves.
//!
//! The fixtures deliberately exercise the hard cases: the same `document_id`
//! split across multiple shards (cross-shard occurrence-index reassignment),
//! empty shards, and single-shard inputs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, read_manifest, sort_entries, write_manifest,
    ManifestEntry, ManifestSignals,
};
use proptest::prelude::*;

static CASE: AtomicU64 = AtomicU64::new(0);

fn attestrum_bin() -> &'static str {
    env!("CARGO_BIN_EXE_attestrum")
}

fn case_dir() -> PathBuf {
    let n = CASE.fetch_add(1, Ordering::Relaxed);
    let mut p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    p.push(format!("merge-byteid-{}-{n}", std::process::id()));
    if p.exists() {
        std::fs::remove_dir_all(&p).expect("clean case dir");
    }
    std::fs::create_dir_all(&p).expect("create case dir");
    p
}

/// A deterministic entry whose `document_id` is keyed on `class` (so equal
/// classes collide into the same digest — drives cross-shard duplicates) with
/// other columns varied by `i` to exercise the full column set under
/// byte-identity.
fn entry(class: u8, i: usize) -> ManifestEntry {
    let mut document_id = [0u8; 32];
    document_id[0] = class;
    let mut sha256 = [0u8; 32];
    sha256[0] = class ^ 0xff;
    ManifestEntry {
        document_id,
        sha256,
        size_bytes: i as u64,
        modality: Modality::Text,
        mime_type: if i % 2 == 0 {
            Some("text/plain".into())
        } else {
            None
        },
        source_url: Some(format!("urn:byteid:{i}")),
        source_type: None,
        source_dataset_id: Some("byteid".into()),
        registered_domain: None,
        license_spdx: Some("CC0-1.0".into()),
        language: if i % 3 == 0 { Some("en".into()) } else { None },
        fetched_at: None,
        signals: ManifestSignals::default(),
        included: true,
        exclusion_reason: None,
        chunk_refs: None,
        input_ordinal: 0,
        occurrence_index: 0,
    }
}

/// Write a shard manifest the way `attestrum_pipeline::build_corpus` does: run
/// the canonical ordering epilogue, then write once.
fn write_shard(path: &Path, mut entries: Vec<ManifestEntry>) {
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);
    write_manifest(path, &entries).expect("write shard manifest");
}

/// In-process reference = the previous load-everything merge: lex-sort input
/// paths, concat, re-run global passes, write once. Returns the reference root.
fn reference_merge(shard_paths: &[PathBuf], out: &Path) -> [u8; 32] {
    let mut sorted = shard_paths.to_vec();
    sorted.sort();
    let mut merged: Vec<ManifestEntry> = Vec::new();
    for p in &sorted {
        merged.extend(read_manifest(p).expect("read shard"));
    }
    assign_input_ordinals(&mut merged);
    assign_occurrence_indices(&mut merged);
    sort_entries(&mut merged);
    write_manifest(out, &merged).expect("write reference merge");
    let leaves: Vec<[u8; 32]> = merged.iter().map(|e| e.document_id).collect();
    attestrum_merkle::merkle_root(&leaves)
}

/// Run the streaming `attestrum merge` binary; panic with stderr on failure.
fn run_merge(shard_paths: &[PathBuf], out: &Path) {
    let mut args: Vec<String> = vec!["merge".into()];
    for p in shard_paths {
        args.push("--inputs".into());
        args.push(p.to_str().unwrap().into());
    }
    args.push("--out".into());
    args.push(out.to_str().unwrap().into());
    let output = Command::new(attestrum_bin())
        .args(&args)
        .output()
        .expect("spawn attestrum merge");
    assert!(
        output.status.success(),
        "merge exited non-zero ({:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Build shards from `(class, shard)` assignments, merge both ways, and assert
/// the streaming merge output is byte-identical to the reference and the merged
/// root matches.
fn assert_byte_identical(num_shards: usize, docs: &[(u8, usize)]) {
    let dir = case_dir();

    // Partition docs into shards by their assigned shard index.
    let mut shard_entries: Vec<Vec<ManifestEntry>> = vec![Vec::new(); num_shards];
    for (i, &(class, shard)) in docs.iter().enumerate() {
        shard_entries[shard % num_shards].push(entry(class, i));
    }

    let mut shard_paths: Vec<PathBuf> = Vec::new();
    for (s, entries) in shard_entries.into_iter().enumerate() {
        let p = dir.join(format!("shard-{s:04}.parquet"));
        write_shard(&p, entries); // empty shards are fine
        shard_paths.push(p);
    }

    let streamed = dir.join("streamed.parquet");
    run_merge(&shard_paths, &streamed);

    let reference = dir.join("reference.parquet");
    let ref_root = reference_merge(&shard_paths, &reference);

    let a = std::fs::read(&reference).expect("read reference");
    let b = std::fs::read(&streamed).expect("read streamed");
    assert_eq!(
        a.len(),
        b.len(),
        "manifest byte length differs: reference {} vs streamed {}",
        a.len(),
        b.len()
    );
    assert!(
        a == b,
        "streaming merge manifest is not byte-identical to the reference merge \
         (num_shards={num_shards}, docs={docs:?})"
    );

    // Independent root cross-check: the binary's merkle.root sibling must match
    // the reference root recomputed from the reference leaves.
    let root_file = dir.join("merkle.root");
    let got = std::fs::read_to_string(&root_file).expect("read merkle.root");
    let got = got.trim();
    let expected = hex_64(&ref_root);
    assert_eq!(
        got, expected,
        "merged merkle.root differs from reference root"
    );

    std::fs::remove_dir_all(&dir).ok();
}

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

// ----------------------------------------------------------------------------
// Explicit adversarial fixtures
// ----------------------------------------------------------------------------

#[test]
fn byte_identical_single_shard() {
    assert_byte_identical(1, &[(0, 0), (1, 0), (0, 0), (2, 0)]);
}

#[test]
fn byte_identical_same_digest_across_shards() {
    // class 0 appears in shards 0, 1, 2 → cross-shard occurrence reassignment.
    assert_byte_identical(3, &[(0, 0), (0, 1), (0, 2), (1, 0), (1, 1)]);
}

#[test]
fn byte_identical_with_empty_shards() {
    // 4 shards declared, docs only land in shards 0 and 2 → shards 1,3 empty.
    assert_byte_identical(4, &[(5, 0), (5, 0), (7, 2), (3, 2), (5, 2)]);
}

// ----------------------------------------------------------------------------
// Randomized differential proptest
// ----------------------------------------------------------------------------

proptest! {
    // Each case spawns the merge binary once; keep the count modest so
    // `cargo test --workspace` stays fast while still fuzzing the orderings.
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn streaming_merge_byte_identical_to_reference(
        num_shards in 1usize..=6,
        // small content alphabet (forces frequent cross-shard duplicates),
        // each doc tagged with a shard assignment
        docs in proptest::collection::vec((0u8..8, 0usize..6), 0..40),
    ) {
        assert_byte_identical(num_shards, &docs);
    }
}
