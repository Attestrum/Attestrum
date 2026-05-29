//! Offline, network-free rejection tests for the public
//! [`attestrum_attest::verify`] entry point.
//!
//! The integrity-audit (2026-05-29, item b/2 + b/3) found that nothing asserts
//! rejection at the `verify()` level — only helper-function unit tests and
//! CLI-structural tests existed. These tests drive `verify()` end-to-end with
//! malformed bundles and assert it REJECTS them.
//!
//! **Scope (honest):** these exercise the rejection gates that fire BEFORE the
//! network/crypto step (`verify.rs` reads + parses the bundle and extracts the
//! cert identity before constructing the Sigstore `Verifier`). They are
//! *structural / pre-crypto* negatives — no OIDC token, no `cosign`, no
//! `#[ignore]`, so they run in the default `cargo test`. The *cryptographic*
//! rejection negatives (flipped signature, wrong manifest against a real signed
//! bundle) require a live signature and live in `cosign_interop.rs` (CI only),
//! because verifying a real bundle needs the Sigstore trust root and a
//! non-expired-at-sign-time Fulcio cert that we deliberately do not commit.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_attest::{verify, AttestrumAttestError, VerifyRequest};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-verify-offline-{test_name}-{n}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

/// A manifest file is required by `VerifyRequest`, but every rejection here
/// fires before `verify()` opens it. Create a real (empty) file so nothing
/// trips on a missing path for the wrong reason.
fn dummy_manifest(root: &Path) -> PathBuf {
    let p = root.join("manifest.parquet");
    std::fs::write(&p, b"").expect("write dummy manifest");
    p
}

fn verify_bundle(bundle: &Path, manifest: &Path) -> Result<(), AttestrumAttestError> {
    verify(VerifyRequest {
        bundle_path: bundle,
        manifest_path: manifest,
        // Permissive policy + offline: prove the rejection is the bundle's
        // fault, not the policy's, and that no network is consulted.
        identity_regex: ".*",
        issuer_regex: ".*",
        offline: true,
    })
    .map(|_| ())
}

#[test]
fn verify_rejects_nonexistent_bundle_file() {
    let root = fresh_root("nonexistent");
    let manifest = dummy_manifest(&root);
    let missing = root.join("does-not-exist.sigstore.json");

    let err = verify_bundle(&missing, &manifest).expect_err("missing bundle must be rejected");
    assert!(
        matches!(err, AttestrumAttestError::Io(_)),
        "missing bundle file should surface as Io, got {err:?}"
    );
}

#[test]
fn verify_rejects_unparseable_bundle() {
    let root = fresh_root("unparseable");
    let manifest = dummy_manifest(&root);
    let bundle = root.join("garbage.sigstore.json");
    std::fs::write(&bundle, b"this is not json {{{").expect("write garbage bundle");

    let err = verify_bundle(&bundle, &manifest).expect_err("garbage bundle must be rejected");
    assert!(
        matches!(err, AttestrumAttestError::Json(_)),
        "unparseable bundle should surface as Json, got {err:?}"
    );
}

#[test]
fn verify_rejects_empty_bundle_file() {
    let root = fresh_root("empty");
    let manifest = dummy_manifest(&root);
    let bundle = root.join("empty.sigstore.json");
    std::fs::write(&bundle, b"").expect("write empty bundle");

    let err = verify_bundle(&bundle, &manifest).expect_err("empty bundle must be rejected");
    assert!(
        matches!(err, AttestrumAttestError::Json(_)),
        "empty bundle should surface as Json, got {err:?}"
    );
}

#[test]
fn verify_rejects_wellformed_json_that_is_not_a_bundle() {
    // Valid JSON, but neither a Sigstore Bundle proto nor a bundle with an
    // extractable leaf cert. Rejected before the network step — either at the
    // proto parse (Json) or at identity extraction (IdentityExtractionFailed);
    // both are correct rejections, so assert is_err without over-specifying.
    let root = fresh_root("not_a_bundle");
    let manifest = dummy_manifest(&root);
    let bundle = root.join("not-a-bundle.sigstore.json");
    std::fs::write(&bundle, br#"{"hello":"world"}"#).expect("write json");

    assert!(
        verify_bundle(&bundle, &manifest).is_err(),
        "well-formed JSON that is not a valid bundle must be rejected"
    );
}

#[test]
fn verify_rejects_bundle_with_no_verification_material() {
    // Shaped a little more like a bundle (has a dsseEnvelope) but carries no
    // verificationMaterial.certificate, so identity extraction cannot succeed.
    // Rejected before any network/crypto.
    let root = fresh_root("no_cert");
    let manifest = dummy_manifest(&root);
    let bundle = root.join("no-cert.sigstore.json");
    std::fs::write(
        &bundle,
        br#"{"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json","dsseEnvelope":{"payload":"e30=","payloadType":"application/vnd.in-toto+json","signatures":[{"sig":"AA=="}]}}"#,
    )
    .expect("write no-cert bundle");

    assert!(
        verify_bundle(&bundle, &manifest).is_err(),
        "bundle without verification material must be rejected"
    );
}
