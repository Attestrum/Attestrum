//! Sprint 3 E8: local mirror of the cross-platform determinism check
//! for `manifest.parquet` bytes.
//!
//! Builds the canonical 1000-document Sprint 3 corpus twice in-process
//! into two fresh workspaces, then asserts:
//!
//!   1. byte-identical `manifest.parquet` across the two runs;
//!   2. identical Merkle root across the two runs;
//!   3. the root equals the canonical Sprint 2 corpus root
//!      `47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73`
//!      (since the document body template is shared with
//!      `crates/attestrum-cas/examples/sprint-2-corpus.rs`).
//!
//! The corpus synthesis block here is DUPLICATED from
//! `crates/attestrum-pipeline/examples/sprint-3-corpus.rs` rather than
//! shared. Rationale: examples and integration-test binaries are
//! separate compilation targets and cannot import each other; the
//! tiny duplication is the established Sprint 2 E9 pattern. **If you
//! change this file, update `examples/sprint-3-corpus.rs` in the same
//! commit — the two share corpus shape, hash algorithm, sort order,
//! and output format by convention.**

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::CasStore;
use attestrum_core::{BuildContext, Modality};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildOutput, ContentSource, CorpusEntry};

const CORPUS_SIZE: u32 = 1000;
const CANONICAL_SPRINT_2_ROOT_HEX: &str =
    "47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73";

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-pipeline-e8-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

/// Synthesise the canonical 1000-doc Sprint 3 corpus. Duplicates the
/// block in `examples/sprint-3-corpus.rs` verbatim per the convention
/// above.
fn synthesize_corpus() -> Vec<CorpusEntry> {
    (0..CORPUS_SIZE)
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
        .collect()
}

fn build_once(test_name: &str) -> (PathBuf, BuildOutput) {
    let root = fresh_root(test_name);
    let cas = CasStore::new(root.join(".attestrum")).expect("CasStore::new");
    let ctx = BuildContext::new(root.clone(), 0);
    let out_dir = root.join(".attestrum").join("manifests");
    let entries = synthesize_corpus();
    let output = build_corpus(&ctx, &cas, &entries, &out_dir).expect("build_corpus");
    (output.manifest_path.clone(), output)
}

#[test]
fn sprint_3_corpus_two_in_process_builds_produce_byte_identical_manifest_and_root() {
    let (manifest_a, out_a) = build_once("two_runs_a");
    let (manifest_b, out_b) = build_once("two_runs_b");

    assert_eq!(
        out_a.merkle_root, out_b.merkle_root,
        "two in-process builds produced different Merkle roots"
    );
    assert_eq!(out_a.leaf_count, out_b.leaf_count);
    assert_eq!(out_a.total_bytes, out_b.total_bytes);

    let bytes_a = fs::read(&manifest_a).expect("read manifest a");
    let bytes_b = fs::read(&manifest_b).expect("read manifest b");
    assert_eq!(bytes_a.len(), bytes_b.len(), "manifest byte lengths differ");
    assert_eq!(
        bytes_a, bytes_b,
        "two in-process builds produced byte-different manifest.parquet"
    );
}

#[test]
fn sprint_3_corpus_root_matches_canonical_sprint_2_root() {
    // The Sprint 3 example shares the document body template with
    // Sprint 2's example, so both pipelines compute the same Merkle
    // root over the same digest list. Locking the canonical value
    // catches inadvertent changes to the corpus shape, the hash
    // algorithm, or the sort order with a single string-mismatch error.
    let (_, out) = build_once("canonical_root");
    let mut hex = String::with_capacity(64);
    for b in &out.merkle_root {
        hex.push_str(&format!("{b:02x}"));
    }
    assert_eq!(
        hex, CANONICAL_SPRINT_2_ROOT_HEX,
        "Sprint 3 corpus root drifted from the canonical Sprint 2 root \
         — either the corpus shape changed (update the constant + Sprint 2 example) \
         or the hash/sort algorithms broke determinism"
    );
}
