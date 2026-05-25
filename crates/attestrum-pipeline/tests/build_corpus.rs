//! Integration tests for `attestrum_pipeline::build_corpus` mapped to the
//! `docs/diagrams/sprint-3/rayon-pipeline.md` test obligations. Test
//! fixtures live under `CARGO_TARGET_TMPDIR` per the pattern established
//! in `crates/attestrum-cas/tests/store.rs`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::{stream_hash, CasStore};
use attestrum_core::{BuildContext, Modality};
use attestrum_manifest::{read_manifest, ManifestSignals};
use attestrum_merkle::{leaf_hash, merkle_root};
use attestrum_pipeline::{build_corpus, BuildError, ContentSource, CorpusEntry};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-pipeline-e4-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

fn fresh_pair(test_name: &str) -> (BuildContext, CasStore, PathBuf) {
    let root = fresh_root(test_name);
    let ctx = BuildContext::new(root.clone(), 0);
    let cas = CasStore::new(root.join(".attestrum")).expect("CasStore::new");
    let out = root.join("out");
    (ctx, cas, out)
}

fn make_entry(source_uri: &str, body: &[u8]) -> CorpusEntry {
    CorpusEntry {
        source_uri: source_uri.into(),
        content: ContentSource::Bytes(body.to_vec()),
        modality: Modality::Text,
        mime_type: Some("text/plain".into()),
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
}

/// xorshift64 — deterministic pseudorandom byte stream. Mirror of the
/// pattern in `crates/attestrum-cas/src/lib.rs:112` so test corpora are
/// reproducible across platforms with no external RNG dep.
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

#[test]
fn empty_corpus_produces_empty_root() {
    let (ctx, cas, out) = fresh_pair("empty_corpus");
    let result = build_corpus(&ctx, &cas, &[], &out).expect("build empty corpus");

    let expected_empty = *blake3::hash(b"").as_bytes();
    assert_eq!(result.merkle_root, expected_empty);
    assert_eq!(result.leaf_count, 0);
    assert_eq!(result.total_bytes, 0);
    assert_eq!(result.manifest_path, out.join("manifest.parquet"));
    assert!(result.manifest_path.exists());

    let rows = read_manifest(&result.manifest_path).expect("read manifest");
    assert!(rows.is_empty(), "empty corpus manifest has 0 rows");
}

#[test]
fn single_document_round_trip() {
    let (ctx, cas, out) = fresh_pair("single_doc");
    let body = b"single doc test content";
    let entries = vec![make_entry("file:///docs/single.txt", body)];

    let result = build_corpus(&ctx, &cas, &entries, &out).expect("build single doc");

    let expected_digest = *blake3::hash(body).as_bytes();
    assert_eq!(result.merkle_root, leaf_hash(&expected_digest));
    assert_eq!(result.leaf_count, 1);
    assert_eq!(result.total_bytes, body.len() as u64);

    let rows = read_manifest(&result.manifest_path).expect("read manifest");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let expected_hash = stream_hash(&body[..]).expect("hash body");
    assert_eq!(row.document_id, expected_hash.blake3);
    assert_eq!(row.sha256, expected_hash.sha256);
    assert_eq!(row.size_bytes, expected_hash.size_bytes);
    assert_eq!(row.source_url.as_deref(), Some("file:///docs/single.txt"));
    assert_eq!(row.input_ordinal, 0);
    assert_eq!(row.occurrence_index, 0);
}

#[test]
fn n_1000_synthetic_documents_seal_deterministically_twice() {
    // 1000 unique-content docs derived from xorshift64; build twice with
    // a fresh output dir each time; assert byte-identical manifest.parquet
    // and identical Merkle root. Local mirror of the cross-platform CI
    // determinism check (E8 extends the matrix to also cmp manifest bytes).
    let (ctx, cas, out_a) = fresh_pair("det_twice_a");
    let (_, cas_b, out_b) = fresh_pair("det_twice_b");

    let mut entries = Vec::with_capacity(1000);
    for i in 0..1000u32 {
        let mut body = vec![0u8; 256];
        xorshift_fill(&mut body, 0xdead_beef_0000_0000 | u64::from(i));
        entries.push(make_entry(&format!("file:///docs/d-{i:04}.bin"), &body));
    }

    let result_a = build_corpus(&ctx, &cas, &entries, &out_a).expect("build a");
    let result_b = build_corpus(&ctx, &cas_b, &entries, &out_b).expect("build b");

    assert_eq!(result_a.merkle_root, result_b.merkle_root);
    assert_eq!(result_a.leaf_count, result_b.leaf_count);
    assert_eq!(result_a.total_bytes, result_b.total_bytes);

    let bytes_a = fs::read(&result_a.manifest_path).expect("read manifest a");
    let bytes_b = fs::read(&result_b.manifest_path).expect("read manifest b");
    assert_eq!(bytes_a.len(), bytes_b.len(), "manifest byte lengths differ");
    assert_eq!(
        bytes_a, bytes_b,
        "manifest bytes differ across two in-process runs of the same corpus"
    );
}

#[test]
fn duplicate_doc_multiset_three_copies_get_indices_012() {
    // Three corpus entries with byte-identical content: occurrence_index
    // 0/1/2 in input order; three adjacent identical BLAKE3 leaves in the
    // sorted leaf list; merkle_root differs from the single-leaf root.
    let (ctx, cas, out) = fresh_pair("multiset_three");
    let body = b"shared content across three input rows";
    let entries = vec![
        make_entry("file:///docs/a.txt", body),
        make_entry("file:///docs/b.txt", body),
        make_entry("file:///docs/c.txt", body),
    ];

    let result = build_corpus(&ctx, &cas, &entries, &out).expect("build triplicate");
    let digest = *blake3::hash(body).as_bytes();

    let rows = read_manifest(&result.manifest_path).expect("read manifest");
    assert_eq!(rows.len(), 3);
    for (rank, row) in rows.iter().enumerate() {
        assert_eq!(row.document_id, digest);
        assert_eq!(
            row.occurrence_index, rank as u32,
            "occurrence_index for adjacent identical-digest rows must be 0,1,2 in canonical order"
        );
    }

    // Sorted leaves are the three identical digests adjacent to each
    // other; merkle_root over those three leaves is the recorded root.
    let leaves = vec![digest; 3];
    assert_eq!(result.merkle_root, merkle_root(&leaves));

    // Multiset binding: three-copy root differs from one-copy root.
    let single_root = merkle_root(&[digest]);
    assert_ne!(
        result.merkle_root, single_root,
        "multiset binding: three copies must produce a different root than one copy"
    );
}

#[test]
fn io_error_in_one_worker_does_not_corrupt_output() {
    // 10 entries; entry index 5 is a Path pointing at a file that does
    // not exist on disk. build_corpus returns Err(BuildError::Io) and the
    // sealed manifest.parquet is NOT created.
    let (ctx, cas, out) = fresh_pair("io_error");
    let bogus_path = out.join("does-not-exist-deliberately.bin");
    assert!(!bogus_path.exists());

    let mut entries: Vec<CorpusEntry> = (0..10u32)
        .map(|i| {
            let body = format!("good content {i}");
            make_entry(&format!("file:///docs/g-{i}.txt"), body.as_bytes())
        })
        .collect();
    entries[5] = CorpusEntry {
        source_uri: "file:///docs/missing.bin".into(),
        content: ContentSource::Path(bogus_path.clone()),
        modality: Modality::Other,
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
    };

    let err = build_corpus(&ctx, &cas, &entries, &out).expect_err("expected Io failure");
    match err {
        BuildError::Io { source_uri, .. } => {
            assert_eq!(source_uri, "file:///docs/missing.bin");
        }
        other => panic!("expected BuildError::Io, got {other:?}"),
    }
    assert!(
        !out.join("manifest.parquet").exists(),
        "no sealed manifest.parquet should be written on worker IO failure"
    );
}

#[test]
fn output_directory_is_created_if_missing() {
    let (ctx, cas, _out_unused) = fresh_pair("auto_mkdir");
    // Build with a fresh subdir that does NOT yet exist.
    let nested = ctx.workspace_root.join("a").join("b").join("c-output");
    assert!(!nested.exists());

    let entries = vec![make_entry("file:///docs/x.txt", b"x")];
    let result = build_corpus(&ctx, &cas, &entries, &nested).expect("build auto-mkdir");

    assert!(nested.is_dir());
    assert!(result.manifest_path.exists());
    assert_eq!(result.manifest_path, nested.join("manifest.parquet"));
}
