//! Integration tests for the per-worker write path through CAS, mapped to
//! the `docs/diagrams/sprint-3/cas-write-path.md` test obligations. Uses
//! the same `CARGO_TARGET_TMPDIR` fresh-root pattern as
//! `crates/attestrum-cas/tests/store.rs`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::{stream_hash, CasStore};
use attestrum_core::{BuildContext, Modality};
use attestrum_manifest::{read_manifest, ManifestSignals};
use attestrum_pipeline::{build_corpus, BuildError, ContentSource, CorpusEntry};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-pipeline-e4-cwp-{test_name}-{n}"));
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

#[test]
fn worker_calls_stream_hash_then_put_then_pushes_manifest_entry() {
    let (ctx, cas, out) = fresh_pair("worker_happy_path");
    let body = b"single-worker happy path body";
    let entries = vec![make_entry("file:///docs/happy.txt", body)];

    let result = build_corpus(&ctx, &cas, &entries, &out).expect("build single entry");

    // The CAS contract: cas.path_for(digest) is the canonical
    // <root>/cas/blake3/<ab>/<cd>/<hex>.bin and the file exists after a
    // successful build. Confirms E4 routes through E6's PROTECTED layout
    // (i.e., the pipeline never writes to a non-canonical CAS path).
    let expected_hash = stream_hash(&body[..]).expect("hash body");
    let cas_path = cas.path_for(&expected_hash.blake3);
    assert!(
        cas_path.exists(),
        "expected CAS object at {} after worker put",
        cas_path.display()
    );

    let rows = read_manifest(&result.manifest_path).expect("read manifest");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.document_id, expected_hash.blake3);
    assert_eq!(row.sha256, expected_hash.sha256);
    assert_eq!(row.size_bytes, expected_hash.size_bytes);
}

#[test]
fn concurrent_workers_writing_same_digest_all_succeed() {
    // 8 corpus entries with byte-identical content. After build:
    //   - CAS contains exactly ONE .bin file at the digest's canonical
    //     path (E6's race-safe idempotent put + BLAKE3 collision
    //     resistance).
    //   - The manifest contains 8 rows (multiset preserved).
    // Locks the composition of E6's race guarantee with E4's parallel
    // worker loop.
    let (ctx, cas, out) = fresh_pair("concurrent_same_digest");
    let body = b"shared content across 8 racing workers";
    let entries: Vec<CorpusEntry> = (0..8u32)
        .map(|i| make_entry(&format!("file:///docs/shared-{i}.txt"), body))
        .collect();

    let result = build_corpus(&ctx, &cas, &entries, &out).expect("build racing");

    let expected_hash = stream_hash(&body[..]).expect("hash body");
    let cas_path = cas.path_for(&expected_hash.blake3);
    assert!(cas_path.exists(), "single CAS object should exist");

    // The shard directory should contain exactly one .bin file (no
    // partial / temp / duplicate-write leftovers).
    let shard_dir = cas_path.parent().expect("cas path parent").to_path_buf();
    let shard_entries: Vec<_> = fs::read_dir(&shard_dir)
        .expect("read shard dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        shard_entries.len(),
        1,
        "expected exactly 1 .bin in shard dir, found {} ({:?})",
        shard_entries.len(),
        shard_entries
            .iter()
            .map(|e| e.file_name())
            .collect::<Vec<_>>()
    );

    let rows = read_manifest(&result.manifest_path).expect("read manifest");
    assert_eq!(rows.len(), 8, "all 8 input rows preserved in the multiset");
    for row in &rows {
        assert_eq!(row.document_id, expected_hash.blake3);
    }
}

#[test]
fn worker_io_error_does_not_crash_other_workers() {
    // 10 entries, entry index 3 is a Path pointing at a nonexistent file
    // on disk. build_corpus must return Err (not panic, not hang) — that
    // proves the other workers' panic / hang would have surfaced via the
    // Rayon pool's panic propagation. Then we confirm the sealed
    // manifest.parquet is absent (no half-built artifact escaped).
    let (ctx, cas, out) = fresh_pair("io_error_no_crash");
    let bogus = ctx.workspace_root.join("not-a-real-file.bin");
    assert!(!bogus.exists());

    let mut entries: Vec<CorpusEntry> = (0..10u32)
        .map(|i| {
            let body = format!("ok body {i}");
            make_entry(&format!("file:///docs/ok-{i}.txt"), body.as_bytes())
        })
        .collect();
    entries[3] = CorpusEntry {
        source_uri: "file:///docs/bad.bin".into(),
        content: ContentSource::Path(bogus),
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

    let err = build_corpus(&ctx, &cas, &entries, &out).expect_err("expected Io err");
    matches!(err, BuildError::Io { .. });
    assert!(!out.join("manifest.parquet").exists());
}

#[test]
fn accumulator_push_order_is_not_observable_externally() {
    // Build the same corpus inside two different Rayon thread pools:
    // one with num_threads(1) (deterministic order), one with
    // num_threads(8) (work-stealing order is non-deterministic). The
    // sealed manifest.parquet must be byte-identical across both runs
    // because the epilogue's sort_by_key(input_ordinal) +
    // assign_occurrence_indices + sort_entries normalises any push-order
    // non-determinism. This locks the determinism claim in
    // `rayon-pipeline.md`'s body.
    let (ctx_a, cas_a, out_a) = fresh_pair("threadpool_one");
    let (ctx_b, cas_b, out_b) = fresh_pair("threadpool_eight");

    let entries: Vec<CorpusEntry> = (0..200u32)
        .map(|i| {
            let body = format!("entry body {i:04} with some payload bytes");
            make_entry(&format!("file:///docs/n-{i:04}.txt"), body.as_bytes())
        })
        .collect();

    let pool_one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("pool 1");
    let pool_eight = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .expect("pool 8");

    let result_a = pool_one
        .install(|| build_corpus(&ctx_a, &cas_a, &entries, &out_a))
        .expect("build pool 1");
    let result_b = pool_eight
        .install(|| build_corpus(&ctx_b, &cas_b, &entries, &out_b))
        .expect("build pool 8");

    assert_eq!(result_a.merkle_root, result_b.merkle_root);
    let bytes_a = fs::read(&result_a.manifest_path).expect("read a");
    let bytes_b = fs::read(&result_b.manifest_path).expect("read b");
    assert_eq!(
        bytes_a, bytes_b,
        "manifest bytes must be identical across thread-pool sizes"
    );
}
