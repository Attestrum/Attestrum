//! Integration tests for the S5-D2 E2 exact-match path
//! (`crates/attestrum-prove/src/lib.rs::prove`).
//!
//! Covers the test obligations enumerated in the plan for E2-E3:
//!
//! E2:
//! - `exact_blake3_match_returns_inclusion_artifact` (E3-updated: now
//!   asserts audit-path length 2 for tree_size=3, not is_empty)
//! - `exact_sha256_match_returns_inclusion_artifact`
//! - `bundle_target_matches_via_blake3`
//! - `ambiguous_match_returns_error`
//! - `statement_predicate_type_is_inclusion_proof_v0_3`
//! - `statement_subject_matches_predicate_matched_subject`
//! - `predicate_round_trips_via_serde_json`
//! - `no_match_panics_with_e6_message`
//! - `huggingface_source_panics_with_e7_message`
//! - `url_source_panics_with_e7_message`
//! - `iscc_target_panics_with_e5_message`
//! - `document_target_panics_with_e5_message`
//! - `corpus_merkle_root_matches_external_compute`
//!
//! E3:
//! - `single_leaf_manifest_returns_empty_audit_path`
//! - `audit_path_verifies_via_external_recompute`
//! - `audit_path_corrupted_proof_rejected_by_verifier`
//!
//! Fixture pattern mirrors `crates/attestrum-pipeline/tests/build_corpus.rs` —
//! per-test directories under `CARGO_TARGET_TMPDIR`, atomically counter-suffixed
//! so parallel tests never collide.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, AttestrumProveError, FingerprintBundle, InclusionProofPredicate, ManifestSource,
    MatchEvidence, ProofKind, ProofTarget, ProveOpts,
};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-prove-e2-{test_name}-{n}"));
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
    }
}

#[test]
fn exact_blake3_match_returns_inclusion_artifact() {
    let root = fresh_root("blake3_hit");
    // Three leaves with document_id = [1;32], [2;32], [3;32]. Sort order
    // is by digest ascending, so post-sort the leaves are at indices
    // 0, 1, 2 in the same numeric order. Querying [2;32] hits index 1.
    let manifest = build_test_manifest(&root, &[1, 2, 3]);

    let artifact = prove(
        ProofTarget::Blake3(digest(2)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("blake3 hit");

    assert_eq!(artifact.kind, ProofKind::Inclusion);
    assert_eq!(artifact.confidence, 1.0);
    assert!(artifact.bundle_path.is_none());
    assert!(artifact.matched_subject.is_some());

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("predicate parse");
    assert_eq!(pred.match_evidence, MatchEvidence::ExactBlake3);
    assert_eq!(pred.leaf_index, 1);
    assert_eq!(pred.tree_size, 3);
    assert_eq!(pred.leaf_count, 3);
    assert_eq!(
        pred.audit_path.len(),
        2,
        "tree_size=3 audit-path length for leaf_index=1 per RFC 6962"
    );
    assert_eq!(pred.hash_algorithm, "blake3-rfc6962");
    pred.validate().expect("predicate validates");
}

#[test]
fn exact_sha256_match_returns_inclusion_artifact() {
    let root = fresh_root("sha256_hit");
    let manifest = build_test_manifest(&root, &[1, 2, 3]);

    // sample_entry's sha256 column is `doc_byte ^ 0xff`. For doc_byte=2,
    // sha256 = [0xfd; 32]. Querying that SHA-256 target hits the same
    // leaf via the SHA-256 column.
    let artifact = prove(
        ProofTarget::Sha256(digest(0xfd)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("sha256 hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("predicate parse");
    assert_eq!(pred.match_evidence, MatchEvidence::ExactSha256);
    assert_eq!(pred.leaf_index, 1);
}

#[test]
fn bundle_target_matches_via_blake3() {
    let root = fresh_root("bundle_hit");
    let manifest = build_test_manifest(&root, &[1, 2, 3]);

    // FingerprintBundle carries BOTH blake3 + sha256 hex. The dispatcher
    // prefers BLAKE3, so this hits via the BLAKE3 column even though
    // the SHA-256 column would also match.
    let bundle = FingerprintBundle {
        schema: String::from("https://attestrum.com/fingerprint/v0.1"),
        modality: Modality::Text,
        blake3: attestrum_core::hex::encode_32(&digest(2)),
        sha256: attestrum_core::hex::encode_32(&digest(0xfd)),
        byte_len: 0,
        text: None,
        image: None,
        iscc: None,
        generated_at: String::from("1970-01-01T00:00:00Z"),
    };

    let artifact = prove(
        ProofTarget::Bundle(Box::new(bundle)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("bundle hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("predicate parse");
    assert_eq!(
        pred.match_evidence,
        MatchEvidence::ExactBlake3,
        "Bundle target prefers BLAKE3 when both columns match"
    );
}

#[test]
fn ambiguous_match_returns_error() {
    let root = fresh_root("ambiguous");
    // Two leaves with identical document_id — manifest's multiset policy
    // permits this; the prover should surface the count rather than
    // pick one arbitrarily.
    let manifest = build_test_manifest(&root, &[5, 5]);

    let err = prove(
        ProofTarget::Blake3(digest(5)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect_err("two leaves with same digest → Ambiguous");

    match err {
        AttestrumProveError::Ambiguous(n) => assert_eq!(n, 2),
        other => panic!("expected Ambiguous(2), got {other:?}"),
    }
}

#[test]
fn statement_predicate_type_is_inclusion_proof_v0_3() {
    let root = fresh_root("predicate_type");
    let manifest = build_test_manifest(&root, &[7]);

    let artifact = prove(
        ProofTarget::Blake3(digest(7)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("single-leaf hit");

    assert_eq!(
        artifact.statement.predicate_type,
        "https://attestrum.com/attestation/inclusion-proof/v0.3"
    );
    assert_eq!(
        artifact.statement.type_uri,
        "https://in-toto.io/Statement/v1"
    );
}

#[test]
fn statement_subject_matches_predicate_matched_subject() {
    let root = fresh_root("subject_match");
    let manifest = build_test_manifest(&root, &[9]);

    let artifact = prove(
        ProofTarget::Blake3(digest(9)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    assert_eq!(artifact.statement.subject.len(), 1);
    assert_eq!(artifact.statement.subject[0], pred.matched_subject);
    assert_eq!(
        Some(pred.matched_subject.clone()),
        artifact.matched_subject,
        "ProofArtifact.matched_subject mirrors the predicate"
    );
}

#[test]
fn predicate_round_trips_via_serde_json() {
    let root = fresh_root("round_trip");
    let manifest = build_test_manifest(&root, &[11, 12]);

    let artifact = prove(
        ProofTarget::Blake3(digest(11)),
        ManifestSource::Local(manifest),
        &default_opts(),
    )
    .expect("hit");

    let original = artifact.statement.predicate.clone();
    let parsed: InclusionProofPredicate =
        serde_json::from_value(original.clone()).expect("first parse");
    let reserialized = serde_json::to_value(&parsed).expect("re-serialize");
    assert_eq!(original, reserialized, "predicate JSON round-trips");
}

#[test]
fn corpus_merkle_root_matches_external_compute() {
    let root = fresh_root("merkle_root");
    let manifest_path = build_test_manifest(&root, &[20, 21, 22, 23]);

    let artifact = prove(
        ProofTarget::Blake3(digest(22)),
        ManifestSource::Local(manifest_path.clone()),
        &default_opts(),
    )
    .expect("hit");

    // Recompute the root via the same attestrum-merkle API the prover
    // used — this validates that the predicate's corpus.merkle_root is
    // the canonical RFC 6962 BLAKE3 root over the manifest's
    // document_id column (in the manifest's sort order).
    let entries = attestrum_manifest::read_manifest(&manifest_path).expect("read");
    let leaves: Vec<[u8; 32]> = entries.iter().map(|e| e.document_id).collect();
    let expected_root_hex = attestrum_core::hex::encode_32(&attestrum_merkle::merkle_root(&leaves));

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    assert_eq!(pred.corpus.merkle_root, expected_root_hex);
    assert_eq!(
        pred.corpus.attestation_digest.blake3,
        "0".repeat(64),
        "E2 placeholder for attestation_digest"
    );
}

#[test]
#[should_panic(expected = "S5-D2 E6")]
fn no_match_panics_with_e6_message() {
    let root = fresh_root("no_match");
    let manifest = build_test_manifest(&root, &[1, 2, 3]);

    // digest(0x88) is not in the manifest → would-be non-inclusion path,
    // which is E6.
    let _ = prove(
        ProofTarget::Blake3(digest(0x88)),
        ManifestSource::Local(manifest),
        &default_opts(),
    );
}

#[test]
#[should_panic(expected = "S5-D2 E7")]
fn huggingface_source_panics_with_e7_message() {
    let _ = prove(
        ProofTarget::Blake3(digest(1)),
        ManifestSource::HuggingFace {
            repo: String::from("allenai/c4"),
            revision: None,
        },
        &default_opts(),
    );
}

#[test]
#[should_panic(expected = "S5-D2 E7")]
fn url_source_panics_with_e7_message() {
    let _ = prove(
        ProofTarget::Blake3(digest(1)),
        ManifestSource::Url(String::from("https://example.com/manifest.parquet")),
        &default_opts(),
    );
}

#[test]
#[should_panic(expected = "S5-D2 E5+")]
fn iscc_target_panics_with_e5_message() {
    let root = fresh_root("iscc_panic");
    let manifest = build_test_manifest(&root, &[1]);

    let _ = prove(
        ProofTarget::Iscc(String::from("ISCC:KACT4EBWK27737D2")),
        ManifestSource::Local(manifest),
        &default_opts(),
    );
}

#[test]
#[should_panic(expected = "S5-D2 E5+")]
fn document_target_panics_with_e5_message() {
    let root = fresh_root("doc_panic");
    let manifest = build_test_manifest(&root, &[1]);

    let _ = prove(
        ProofTarget::Document(PathBuf::from("/dev/null")),
        ManifestSource::Local(manifest),
        &default_opts(),
    );
}

// ============================================================================
// S5-D2 E3 — real audit-path tests
// ============================================================================

/// Single-leaf manifest edge case: the leaf hash IS the root, so the
/// audit-path is empty by definition (RFC 6962 + `MerkleTree::audit_path`
/// at `crates/attestrum-merkle/src/lib.rs:238-240`).
#[test]
fn single_leaf_manifest_returns_empty_audit_path() {
    let root = fresh_root("single_leaf");
    let manifest_path = build_test_manifest(&root, &[42]);

    let artifact = prove(
        ProofTarget::Blake3(digest(42)),
        ManifestSource::Local(manifest_path),
        &default_opts(),
    )
    .expect("single-leaf hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");

    assert!(
        pred.audit_path.is_empty(),
        "single-leaf tree: audit_path is [] because leaf hash IS the root"
    );
    assert_eq!(pred.tree_size, 1);
    assert_eq!(pred.leaf_index, 0);

    // Verify-via-recompute even for the trivial case.
    let root_bytes = attestrum_core::hex::decode_32(&pred.corpus.merkle_root).expect("root hex");
    let leaf_bytes = attestrum_core::hex::decode_32(&pred.leaf_hash).expect("leaf hex");
    assert!(
        attestrum_merkle::verify_audit_path(&root_bytes, &leaf_bytes, 0, 1, &[]),
        "single-leaf audit-path verifies"
    );
}

/// Headline E3 property: the predicate is cryptographically self-contained.
/// An external verifier — using ONLY the predicate's `{leaf_hash,
/// leaf_index, tree_size, audit_path, corpus.merkle_root}` fields — can
/// re-derive the corpus's Merkle root via
/// `attestrum_merkle::verify_audit_path` without reading the original
/// manifest. This is the test that proves E3 actually shipped: an empty
/// audit_path would fail verification for tree_size > 1.
#[test]
fn audit_path_verifies_via_external_recompute() {
    let root = fresh_root("audit_verify");
    // 7 leaves → unbalanced tree exercising the RFC 6962 odd-count
    // carry-up rule that `MerkleTree::audit_path` implements.
    let manifest_path = build_test_manifest(&root, &[1, 2, 3, 4, 5, 6, 7]);

    let artifact = prove(
        ProofTarget::Blake3(digest(4)),
        ManifestSource::Local(manifest_path),
        &default_opts(),
    )
    .expect("middle leaf hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");

    let root_bytes = attestrum_core::hex::decode_32(&pred.corpus.merkle_root).expect("root hex");
    let leaf_bytes = attestrum_core::hex::decode_32(&pred.leaf_hash).expect("leaf hex");
    let path_bytes: Vec<[u8; 32]> = pred
        .audit_path
        .iter()
        .map(|hex| attestrum_core::hex::decode_32(hex).expect("audit-path hex"))
        .collect();

    assert!(
        !path_bytes.is_empty(),
        "tree_size=7 must produce a non-empty audit-path"
    );
    assert!(
        attestrum_merkle::verify_audit_path(
            &root_bytes,
            &leaf_bytes,
            pred.leaf_index as usize,
            pred.tree_size as usize,
            &path_bytes,
        ),
        "audit-path verifies against the predicate's claimed root"
    );
}

/// Negative property: flipping one byte in the audit-path's first entry
/// makes the verifier reject. Confirms `verify_audit_path` actually
/// exercises the proof (not just the leaf+root pair). Cheap and
/// high-signal.
#[test]
fn audit_path_corrupted_proof_rejected_by_verifier() {
    let root = fresh_root("audit_corrupt");
    let manifest_path = build_test_manifest(&root, &[1, 2, 3, 4, 5, 6, 7]);

    let artifact = prove(
        ProofTarget::Blake3(digest(4)),
        ManifestSource::Local(manifest_path),
        &default_opts(),
    )
    .expect("hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");

    let root_bytes = attestrum_core::hex::decode_32(&pred.corpus.merkle_root).expect("root hex");
    let leaf_bytes = attestrum_core::hex::decode_32(&pred.leaf_hash).expect("leaf hex");
    let mut path_bytes: Vec<[u8; 32]> = pred
        .audit_path
        .iter()
        .map(|hex| attestrum_core::hex::decode_32(hex).expect("audit-path hex"))
        .collect();

    assert!(!path_bytes.is_empty());
    // Flip one bit in the first audit-path element.
    path_bytes[0][0] ^= 0x01;

    assert!(
        !attestrum_merkle::verify_audit_path(
            &root_bytes,
            &leaf_bytes,
            pred.leaf_index as usize,
            pred.tree_size as usize,
            &path_bytes,
        ),
        "corrupted audit-path must be rejected"
    );
}
