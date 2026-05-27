//! Integration tests for the S5-D2 E4 DSSE-sign path
//! (`crates/attestrum-prove/src/lib.rs::prove` with `opts.sign=true`,
//! `opts.corpus_bundle_path`).
//!
//! Three tests:
//!
//! - `sign_true_without_oidc_token_returns_sign_error` — non-ignored,
//!   no network. Confirms the caller-contract check: when `opts.sign=true`
//!   and `opts.oidc_id_token=None`, `prove()` returns
//!   `AttestrumProveError::Sign(AttestrumAttestError::SigstoreIdentityToken(_))`.
//! - `corpus_bundle_path_populates_attestation_digest` — non-ignored,
//!   no network. When `opts.corpus_bundle_path = Some(fixture)`,
//!   `pred.corpus.attestation_digest` equals the BLAKE3 + SHA-256 of
//!   the fixture file's bytes (via `attestrum_cas::stream_hash_path`).
//! - `signed_prove_emits_verifiable_bundle` — `#[ignore]`d. Requires
//!   `SIGSTORE_ID_TOKEN` env var + network access to Fulcio + Rekor +
//!   TUF. Runs only via `cargo test ... -- --ignored` (intended for
//!   `.github/workflows/cosign-interop.yml`-style execution). Asserts
//!   `prove()` with `opts.sign=true` emits a Bundle file at
//!   `<workspace>/prove/inclusion-proof.sigstore.json` that parses as
//!   JSON. End-to-end `attestrum_attest::verify` and cosign-binary
//!   round-trips are deferred to a follow-on CI step (require real
//!   corpus bytes whose SHA-256 matches the bundle's subject digest,
//!   which the synthetic fixtures here don't satisfy).
//!
//! Fixture pattern mirrors `crates/attestrum-prove/tests/exact_match.rs`
//! and `crates/attestrum-attest/tests/cosign_interop.rs` — per-test
//! directories under `CARGO_TARGET_TMPDIR`, atomically counter-suffixed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, AttestrumAttestError, AttestrumProveError, InclusionProofPredicate, ManifestSource,
    ProofTarget, ProveOpts,
};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-prove-e4-{test_name}-{n}"));
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

#[test]
fn sign_true_without_oidc_token_returns_sign_error() {
    let root = fresh_root("contract_violation");
    let manifest = build_test_manifest(&root, &[1, 2, 3]);

    let opts = ProveOpts {
        sign: true, // contract violation: sign=true requires oidc_id_token=Some
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: None,
    };

    let err = prove(
        ProofTarget::Blake3(digest(2)),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect_err("sign=true + oidc_id_token=None must error");

    match err {
        AttestrumProveError::Sign(AttestrumAttestError::SigstoreIdentityToken(msg)) => {
            assert!(
                msg.contains("oidc_id_token must be Some"),
                "error message must explain the contract: got {msg:?}"
            );
        }
        other => panic!("expected Sign(SigstoreIdentityToken(_)), got {other:?}"),
    }
}

#[test]
fn corpus_bundle_path_populates_attestation_digest() {
    let root = fresh_root("corpus_digest");
    let manifest = build_test_manifest(&root, &[10, 11, 12]);

    // Write a small fixture file standing in for a corpus bundle. Real
    // corpus bundles are Sigstore Bundle v0.3 JSON; for this test we
    // only need a file with deterministic bytes so we can verify the
    // digest computation independently.
    let corpus_bundle = root.join("corpus.bundle.json");
    let fixture_bytes = br#"{"placeholder":"S5-D2 E4 test fixture for corpus_bundle_path"}"#;
    std::fs::write(&corpus_bundle, fixture_bytes).expect("write corpus fixture");

    let opts = ProveOpts {
        sign: false,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: Some(corpus_bundle.clone()),
        cas_root: None,
    };

    let artifact = prove(
        ProofTarget::Blake3(digest(11)),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect("hit with corpus_bundle_path");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");

    // Independently compute the expected digests via the same helper
    // prove() used. Asserting non-zero AND equal-to-stream-hash gives
    // strong evidence the field is wired correctly.
    let expected = attestrum_cas::stream_hash_path(&corpus_bundle).expect("hash fixture");
    let expected_b3 = attestrum_core::hex::encode_32(&expected.blake3);
    let expected_s256 = attestrum_core::hex::encode_32(&expected.sha256);

    assert_eq!(pred.corpus.attestation_digest.blake3, expected_b3);
    assert_eq!(pred.corpus.attestation_digest.sha256, expected_s256);
    assert_ne!(
        pred.corpus.attestation_digest.blake3,
        "0".repeat(64),
        "attestation_digest must not be the placeholder when corpus_bundle_path is Some"
    );

    // proof_generated_at is also populated at E4 — confirm here as a
    // bundled sanity check (matches the assertion added to
    // exact_match.rs::corpus_merkle_root_matches_external_compute).
    assert!(
        pred.proof_generated_at.is_some(),
        "proof_generated_at populated unconditionally at E4"
    );
}

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN + network to Fulcio + Rekor + TUF; runs in cosign-interop CI only"]
fn signed_prove_emits_verifiable_bundle() {
    let token = match env_token() {
        Some(t) => t,
        None => return, // skip silently per the cosign_interop pattern
    };

    let root = fresh_root("signed_prove");
    let manifest = build_test_manifest(&root, &[20, 21, 22, 23, 24]);
    let workspace = root.join("ws");
    std::fs::create_dir_all(&workspace).expect("create workspace");

    let opts = ProveOpts {
        sign: true,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: Some(token),
        workspace: Some(workspace.clone()),
        corpus_bundle_path: None, // attestation_digest stays zeros-hex for this test
        cas_root: None,
    };

    let artifact = prove(
        ProofTarget::Blake3(digest(22)),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect("signed prove against real Fulcio + Rekor");

    let bundle_path = artifact
        .bundle_path
        .expect("opts.sign=true must populate bundle_path");

    assert!(
        bundle_path.starts_with(&workspace),
        "bundle written under opts.workspace: bundle_path={bundle_path:?} \
         workspace={workspace:?}"
    );
    assert!(
        bundle_path.exists(),
        "bundle file must exist at {bundle_path:?}"
    );
    assert_eq!(
        bundle_path.file_name().and_then(|s| s.to_str()),
        Some("inclusion-proof.sigstore.json")
    );

    // Sanity: the file is well-formed JSON. End-to-end verify-via-cosign
    // and verify-via-attestrum-attest round-trips require real corpus
    // bytes whose SHA-256 matches the bundle's subject digest — the
    // synthetic fixtures here don't satisfy that. Deferred to a
    // follow-on CI step that builds a real corpus first.
    let bytes = std::fs::read(&bundle_path).expect("read bundle");
    let _: serde_json::Value = serde_json::from_slice(&bytes).expect("bundle is valid JSON");
}

fn env_token() -> Option<String> {
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(t) if !t.is_empty() => Some(t),
        _ => {
            eprintln!(
                "sign_integration: SIGSTORE_ID_TOKEN not set — skipping. This test \
                 only runs in the dedicated CI workflow where the GHA OIDC exchange \
                 exports the token."
            );
            None
        }
    }
}
