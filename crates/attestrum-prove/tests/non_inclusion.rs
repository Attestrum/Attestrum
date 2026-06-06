//! Integration tests for the S5-D2 E6 non-inclusion path
//! (`crates/attestrum-prove/src/lib.rs::prove` →
//! `dispatch_non_inclusion`).
//!
//! Pins the boundary-case contract of
//! `attestrum_attest::NonInclusionProofPredicate` against the
//! PROTECTED-extension helper
//! `attestrum_merkle::find_adjacent_leaves`. Detailed coverage:
//!
//! - Boundary cases: Interior, BeforeFirst, AfterLast, Empty.
//! - Neighbor audit paths round-trip through
//!   `attestrum_merkle::verify_audit_path`.
//! - Predicate fields (query_key, sorted_assertion, predicate_type
//!   URI, JSON round-trip).
//! - Sha256 + fuzzy non-inclusion deferral to v0.2 (typed-error
//!   responses, not panics).
//! - Signed non-inclusion emits `non-inclusion-proof.sigstore.json`
//!   (`#[ignore]`d — requires `SIGSTORE_ID_TOKEN`).
//! - Duplicate-adjacent-leaf handling per the v0.1
//!   `SortedAssertion.duplicate_leaf_policy` convention.
//!
//! Fixture pattern mirrors `tests/exact_match.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, AttestrumProveError, BoundaryCase, ManifestSource, NonInclusionProofPredicate,
    PerceptualHashes, ProofKind, ProofTarget, ProveOpts, SortedAssertion,
    NON_INCLUSION_PROOF_PREDICATE_TYPE,
};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-prove-e6-{test_name}-{n}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

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
        input_ordinal: 0,
        occurrence_index: 0,
    }
}

fn build_test_manifest(root: &Path, doc_bytes: &[u8]) -> PathBuf {
    let mut entries: Vec<ManifestEntry> = doc_bytes.iter().map(|b| sample_entry(*b)).collect();
    assign_input_ordinals(&mut entries);
    sort_entries(&mut entries);
    assign_occurrence_indices(&mut entries);
    let manifest_path = root.join("manifest.parquet");
    write_manifest(&manifest_path, &entries).expect("write_manifest");
    manifest_path
}

fn default_opts() -> ProveOpts {
    ProveOpts {
        sign: false,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: None,
        no_index: false,
    }
}

fn parse_predicate(artifact_predicate: &serde_json::Value) -> NonInclusionProofPredicate {
    serde_json::from_value(artifact_predicate.clone()).expect("predicate parses")
}

// ============================================================================
// Boundary-case coverage
// ============================================================================

#[test]
fn interior_non_inclusion_returns_both_neighbors() {
    let root = fresh_root("interior");
    let manifest = build_test_manifest(&root, &[0x10, 0x30, 0x50]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    assert_eq!(artifact.kind, ProofKind::NonInclusion);
    assert_eq!(artifact.confidence, 1.0);

    let pred = parse_predicate(&artifact.statement.predicate);
    assert_eq!(pred.boundary_case, BoundaryCase::Interior);
    let left = pred.left_neighbor.expect("Interior has left neighbor");
    let right = pred.right_neighbor.expect("Interior has right neighbor");
    assert_eq!(left.leaf_index, 0);
    assert_eq!(right.leaf_index, 1);
    assert_eq!(left.leaf_hash, "10".repeat(32));
    assert_eq!(right.leaf_hash, "30".repeat(32));
}

#[test]
fn before_first_non_inclusion_returns_right_only() {
    let root = fresh_root("before_first");
    let manifest = build_test_manifest(&root, &[0x10, 0x20, 0x30]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x01)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    assert_eq!(pred.boundary_case, BoundaryCase::BeforeFirst);
    assert!(
        pred.left_neighbor.is_none(),
        "BeforeFirst has no left neighbor"
    );
    let right = pred.right_neighbor.expect("BeforeFirst has right neighbor");
    assert_eq!(right.leaf_index, 0);
    assert_eq!(right.leaf_hash, "10".repeat(32));
}

#[test]
fn after_last_non_inclusion_returns_left_only() {
    let root = fresh_root("after_last");
    let manifest = build_test_manifest(&root, &[0x10, 0x20]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0xff)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    assert_eq!(pred.boundary_case, BoundaryCase::AfterLast);
    let left = pred.left_neighbor.expect("AfterLast has left neighbor");
    assert!(
        pred.right_neighbor.is_none(),
        "AfterLast has no right neighbor"
    );
    assert_eq!(left.leaf_index, 1);
    assert_eq!(left.leaf_hash, "20".repeat(32));
}

#[test]
fn empty_manifest_returns_invalid_manifest_error() {
    let root = fresh_root("empty");
    let manifest = build_test_manifest(&root, &[]);

    let err = prove(
        ProofTarget::Blake3(digest(0x42)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect_err("empty manifest is invalid for non-inclusion");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("empty manifest"),
                "expected 'empty manifest' in error message, got: {msg}"
            );
        }
        other => panic!("expected InvalidManifest, got: {other:?}"),
    }
}

// ============================================================================
// Neighbor audit-path round-trip — the verifier-side correctness pin
// ============================================================================

#[test]
fn non_inclusion_neighbor_audit_paths_verify_against_corpus_root() {
    let root = fresh_root("neighbor_verify");
    let manifest = build_test_manifest(&root, &[0x10, 0x30, 0x50, 0x70]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x40)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    let corpus_root_bytes = attestrum_core::hex::decode_32(&pred.corpus.merkle_root)
        .expect("corpus.merkle_root decodes as hex");
    let tree_size = pred.tree_size as usize;

    for (label, neighbor_opt) in [
        ("left", pred.left_neighbor.as_ref()),
        ("right", pred.right_neighbor.as_ref()),
    ] {
        let n = neighbor_opt.unwrap_or_else(|| panic!("Interior must have a {label} neighbor"));
        let leaf_bytes = attestrum_core::hex::decode_32(&n.leaf_hash)
            .expect("neighbor.leaf_hash decodes as hex");
        let audit: Vec<[u8; 32]> = n
            .inclusion_proof_audit_path
            .iter()
            .map(|h| attestrum_core::hex::decode_32(h).expect("audit step decodes"))
            .collect();
        assert!(
            attestrum_merkle::verify_audit_path(
                &corpus_root_bytes,
                &leaf_bytes,
                n.leaf_index as usize,
                tree_size,
                &audit,
            ),
            "{label} neighbor audit_path failed to verify against corpus root"
        );
    }
}

// ============================================================================
// Predicate-field contract
// ============================================================================

#[test]
fn query_key_matches_target_hex() {
    let root = fresh_root("query_key");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);
    let target = digest(0x20);

    let artifact = prove(
        ProofTarget::Blake3(target),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    assert_eq!(pred.query_key, attestrum_core::hex::encode_32(&target));
}

#[test]
fn sorted_assertion_documents_v0_1_conventions() {
    let root = fresh_root("sorted_assertion");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    assert_eq!(
        pred.sorted_assertion.ordering,
        SortedAssertion::ORDERING_V0_1
    );
    assert_eq!(
        pred.sorted_assertion.adjacency_invariant,
        SortedAssertion::ADJACENCY_INVARIANT_V0_1
    );
    assert!(
        !pred.sorted_assertion.duplicate_leaf_policy.is_empty(),
        "duplicate_leaf_policy must document the v0.1 multiset convention"
    );
}

#[test]
fn statement_predicate_type_is_non_inclusion_proof_v0_3() {
    let root = fresh_root("predicate_type");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    assert_eq!(
        artifact.statement.predicate_type, NON_INCLUSION_PROOF_PREDICATE_TYPE,
        "Statement predicateType must be the PROTECTED v0.3 URI"
    );
}

#[test]
fn synthetic_absent_subject_carries_target_hex() {
    let root = fresh_root("absent_subject");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);
    let target = digest(0x20);
    let target_hex = attestrum_core::hex::encode_32(&target);

    let artifact = prove(
        ProofTarget::Blake3(target),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    assert_eq!(artifact.statement.subject.len(), 1);
    let subject = &artifact.statement.subject[0];
    assert_eq!(subject.name, format!("absent:{target_hex}"));
    assert_eq!(subject.digest.blake3, target_hex);
}

#[test]
fn predicate_round_trips_via_serde_json() {
    let root = fresh_root("round_trip");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("non-inclusion path returns Ok");

    let pred = parse_predicate(&artifact.statement.predicate);
    let json = serde_json::to_string(&pred).expect("serialize");
    let back: NonInclusionProofPredicate = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(pred, back);
}

// ============================================================================
// v0.2 deferral — typed-error responses, not panics
// ============================================================================

#[test]
fn sha256_non_inclusion_is_deferred_to_v0_2() {
    let root = fresh_root("sha256_defer");
    let manifest = build_test_manifest(&root, &[0x10, 0x20]);

    let err = prove(
        // [0xab;32] has no match in either column.
        ProofTarget::Sha256([0xab; 32]),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect_err("Sha256 non-inclusion is deferred");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("Sha256 non-inclusion is v0.2 work"),
                "expected v0.2 deferral message, got: {msg}"
            );
        }
        other => panic!("expected InvalidManifest, got: {other:?}"),
    }
}

#[test]
fn iscc_non_inclusion_is_deferred_to_v0_2() {
    // Empty-manifest fixture: dispatch_iscc iterates entries
    // unconditionally and reads each leaf's blob from CAS first; with
    // a non-empty manifest + empty CAS we'd trip an unrelated CAS-miss
    // error before reaching the no-match deferral branch. Empty
    // entries trivially skip the scan and emit the same fuzzy-deferral
    // message the v0.2-target real-corpus path will emit, which is
    // exactly the contract pinned here.
    let root = fresh_root("iscc_defer");
    let manifest = build_test_manifest(&root, &[]);
    let cas_root = root.join("cas");
    attestrum_cas::CasStore::new(&cas_root).expect("open empty CAS");

    let mut opts = default_opts();
    opts.cas_root = Some(cas_root);

    let err = prove(
        ProofTarget::Iscc("ISCC:KACT4EBWK27737D2".into()),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect_err("fuzzy non-inclusion is deferred");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("fuzzy non-inclusion is v0.2 work"),
                "expected fuzzy-deferral message, got: {msg}"
            );
        }
        other => panic!("expected InvalidManifest, got: {other:?}"),
    }
}

#[test]
fn perceptual_non_inclusion_is_deferred_to_v0_2() {
    let root = fresh_root("perceptual_defer");
    let manifest = build_test_manifest(&root, &[0x10, 0x20]);
    let cas_root = root.join("cas");
    attestrum_cas::CasStore::new(&cas_root).expect("open empty CAS");

    let mut opts = default_opts();
    opts.cas_root = Some(cas_root);

    let err = prove(
        ProofTarget::Perceptual(PerceptualHashes {
            phash: [0u8; 8],
            blockhash: [0u8; 8],
        }),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect_err("fuzzy non-inclusion is deferred");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("fuzzy non-inclusion is v0.2 work"),
                "expected fuzzy-deferral message, got: {msg}"
            );
        }
        other => panic!("expected InvalidManifest, got: {other:?}"),
    }
}

// ============================================================================
// Signed non-inclusion (gated on SIGSTORE_ID_TOKEN — mirrors E4)
// ============================================================================

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN env var + Fulcio network access (mirrors E4 signed inclusion gate)"]
fn signed_non_inclusion_emits_bundle() {
    let root = fresh_root("signed_non_inclusion");
    let manifest = build_test_manifest(&root, &[0x10, 0x30]);
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace dir");
    let oidc = std::env::var("SIGSTORE_ID_TOKEN").expect("SIGSTORE_ID_TOKEN required");

    let opts = ProveOpts {
        sign: true,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: Some(oidc),
        workspace: Some(workspace.clone()),
        corpus_bundle_path: None,
        cas_root: None,
        no_index: false,
    };

    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect("signed non-inclusion returns Ok");

    assert_eq!(artifact.kind, ProofKind::NonInclusion);
    let bundle_path = artifact
        .bundle_path
        .expect("sign=true populates bundle_path");
    assert!(
        bundle_path
            .to_string_lossy()
            .ends_with("non-inclusion-proof.sigstore.json"),
        "expected non-inclusion-proof.sigstore.json filename, got: {bundle_path:?}"
    );
    assert!(bundle_path.exists(), "bundle file should exist on disk");
}
