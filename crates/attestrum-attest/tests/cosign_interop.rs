//! Sprint 4 E4.5 — `cosign verify-blob-attestation --new-bundle-format`
//! third-party interop.
//!
//! Closes Sprint 4 acceptance per `PATH-A-BRIEF.md` §1.5 + Part 6 Sprint 4:
//! a third party with zero Attestrum installed must be able to verify our
//! bundles using `cosign` alone. This test signs ONE real bundle against the
//! Sigstore public-good roots, self-verifies via [`attestrum_attest::verify`]
//! as a sanity gate, shells out to `cosign` to confirm the bundle round-trips
//! (positive path), then reuses that same signed bundle to assert that BOTH
//! attestrum's verifier AND cosign **reject** four forgeries — flipped
//! signature, wrong manifest, identity-regex mismatch, and a truncated bundle
//! (negative path). A signature is only meaningful if verification rejects
//! tampered inputs; the negatives close the integrity-audit 2026-05-29 "no
//! crypto-rejection test exists anywhere" gap.
//!
//! **Fail-not-skip preconditions** (integrity-audit 2026-05-29 item b/1 — a
//! silent skip-as-pass was a false trust signal):
//!
//! - `SIGSTORE_ID_TOKEN` env var must be set (sourced from the GHA OIDC
//!   exchange in `.github/workflows/cosign-interop.yml`).
//! - `cosign` binary must be on PATH (installed via the workflow's
//!   `sigstore/cosign-installer@v3` step).
//!
//! Missing either precondition → **PANIC (the test FAILS, not skips)**. Network
//! failures, sign failures, self-verify failures, and cosign mismatches all
//! fail the test too.
//!
//! `#[ignore]`d by default (it needs network + an OIDC token) — `cargo test
//! --workspace` lists but does not execute it, so the default suite is
//! unaffected by the fail-not-skip change. The dedicated workflow runs it via
//! `cargo test --workspace --test cosign_interop -- --include-ignored cosign_interop`,
//! where the missing-precondition panic now makes a tokenless CI run fail loudly
//! instead of green-skipping.

use std::path::Path;
use std::process::Command;

use base64::Engine;

use attestrum_attest::{
    sign as attest_sign, verify as attest_verify, verify_statement, AttestrumAttestError,
    DeterminismFields, DigestMap, InTotoStatement, LicensingPosture, ManifestRef, RulesetMode,
    SignRequest, SignalCoverage, Subject, TrainingCorpusPredicate, VerifiedAttestation,
    VerifyRequest, MODEL_BINDING_PREDICATE_TYPE, TRAINING_CORPUS_PREDICATE_TYPE,
};
use attestrum_cas::{stream_hash, CasStore};
use attestrum_core::BuildContext;
use attestrum_pipeline::build_corpus;

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN + cosign on PATH + network; runs in .github/workflows/cosign-interop.yml only"]
fn cosign_interop() {
    let oidc_token = require_token();
    require_cosign();

    let tmpdir = std::env::temp_dir().join(format!(
        "attestrum-attest-cosign-interop-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmpdir).expect("tmpdir create");

    // Empty-corpus build through the production pipeline. Produces a real
    // `manifest.parquet` whose bytes match the CLI's sign-path output for
    // an empty input, so the bundle subject digest is computed over the
    // same shape of artifact a real `attestrum sign` would produce.
    let workspace = tmpdir.join("workspace");
    let ctx = BuildContext::new(workspace.clone(), 1_748_109_600);
    let cas =
        CasStore::new(workspace.join("cas")).expect("CasStore::new under tmpdir should succeed");
    let output_dir = workspace.join("out");
    let build_output = build_corpus(&ctx, &cas, &[], &output_dir)
        .expect("build_corpus over empty entries should succeed");
    let manifest_path = build_output.manifest_path.clone();

    // Hash the manifest bytes via the same stream-hash helper the pipeline
    // uses — single source of truth for BLAKE3 + SHA-256 + size.
    let manifest_bytes = std::fs::read(&manifest_path).expect("read manifest.parquet");
    let hash = stream_hash(&manifest_bytes[..]).expect("stream_hash manifest.parquet");
    let digest_set = DigestMap {
        blake3: hex_64(&hash.blake3),
        sha256: hex_64(&hash.sha256),
    };

    let predicate = TrainingCorpusPredicate {
        attestrum_version: "0.0.1".to_string(),
        builder_version: "attestrum-attest-cosign-interop-test/0.0.1".to_string(),
        built_at: "2025-05-24T18:00:00Z".to_string(),
        determinism: DeterminismFields {
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            seed: "1748109600".to_string(),
            manifest_schema_version: "2".to_string(),
        },
        manifest: ManifestRef {
            uri: format!("file://{}", manifest_path.display()),
            digest_set: digest_set.clone(),
            row_count: build_output.leaf_count as u64,
            byte_count: hash.size_bytes,
        },
        merkle_root: hex_64(&build_output.merkle_root),
        merkle_algorithm: "blake3-rfc6962".to_string(),
        ruleset_mode: RulesetMode::Strict,
        ruleset_id: "attestrum-default".to_string(),
        ruleset_version: "v0.1.0".to_string(),
        signal_coverage: SignalCoverage::default(),
        licensing_posture: LicensingPosture::Undisclosed,
        license_inventory: vec![],
        takedown_contact: None,
        dataset_homepage: None,
        publication_intent: None,
        total_compute: None,
        training_cost: None,
        model_name: None,
    };
    let predicate_value = serde_json::to_value(&predicate).expect("predicate to JSON value");

    let subject = Subject {
        name: "manifest.parquet".to_string(),
        digest: digest_set,
    };
    let statement = InTotoStatement::new(
        TRAINING_CORPUS_PREDICATE_TYPE,
        vec![subject],
        predicate_value,
    );
    let canonical_payload = statement
        .canonical_json()
        .expect("statement canonical_json");

    // Sign against Sigstore public-good (Fulcio + Rekor v2 + TUF).
    let bundle_path = tmpdir.join("bundle.sigstore.json");
    let signed = attest_sign(SignRequest {
        statement_payload: canonical_payload.as_bytes(),
        bundle_output_path: &bundle_path,
        oidc_id_token: oidc_token,
    })
    .expect("attestrum_attest::sign against public-good");
    assert!(
        bundle_path.exists(),
        "bundle file should exist after sign returned Ok"
    );

    // Self-verify gate. The identity + issuer regexes are plucked from the
    // SAN we actually extracted (tactical decision E) — escape and anchor
    // the literal values rather than predicting a GHA-workflow URL
    // pattern. If our verifier rejects what we just signed, surface here
    // before delegating to cosign.
    let identity_pattern = format!("^{}$", regex::escape(&signed.identity));
    let issuer_pattern = format!("^{}$", regex::escape(&signed.oidc_issuer));
    attest_verify(VerifyRequest {
        bundle_path: &bundle_path,
        manifest_path: &manifest_path,
        identity_regex: &identity_pattern,
        issuer_regex: &issuer_pattern,
        offline: false,
        expected_predicate_type: None,
    })
    .expect("attestrum_attest::verify self-verify sanity gate");

    // verify() widening (Commit 3): the lower-level verify_statement() must
    // accept this training-corpus bundle when the expected predicate type is
    // pinned explicitly to training-corpus...
    verify_statement(VerifyRequest {
        bundle_path: &bundle_path,
        manifest_path: &manifest_path,
        identity_regex: &identity_pattern,
        issuer_regex: &issuer_pattern,
        offline: false,
        expected_predicate_type: Some(TRAINING_CORPUS_PREDICATE_TYPE),
    })
    .expect("verify_statement accepts a training-corpus bundle when expected");

    // ...and REJECT it (post-crypto, at the predicate-type gate) when a
    // different family is expected. This proves the widening gate discriminates
    // by predicateType on a real signed bundle, not just structurally.
    let wrong_family = verify_statement(VerifyRequest {
        bundle_path: &bundle_path,
        manifest_path: &manifest_path,
        identity_regex: &identity_pattern,
        issuer_regex: &issuer_pattern,
        offline: false,
        expected_predicate_type: Some(MODEL_BINDING_PREDICATE_TYPE),
    });
    assert!(
        matches!(
            wrong_family,
            Err(AttestrumAttestError::PredicateValidationFailed(_))
        ),
        "verify_statement must reject a training-corpus bundle when model-binding is expected, got {wrong_family:?}"
    );

    // Shell out to cosign. The OIDC issuer is passed as a literal
    // (cosign supports both `--certificate-oidc-issuer` literal and
    // `-regexp` forms; the issuer is a stable URL so the literal form is
    // simplest). The identity goes through `-regexp` with the same
    // escape+anchor pattern the self-verify gate consumed.
    let cosign = Command::new("cosign")
        .arg("verify-blob-attestation")
        .arg("--new-bundle-format")
        .arg("--bundle")
        .arg(&bundle_path)
        .arg("--certificate-identity-regexp")
        .arg(&identity_pattern)
        .arg("--certificate-oidc-issuer")
        .arg(&signed.oidc_issuer)
        .arg(&manifest_path)
        .output()
        .expect("spawn cosign verify-blob-attestation");

    let stdout = String::from_utf8_lossy(&cosign.stdout);
    let stderr = String::from_utf8_lossy(&cosign.stderr);
    assert!(
        cosign.status.success(),
        "cosign verify-blob-attestation failed: exit={:?}\nstdout={stdout}\nstderr={stderr}",
        cosign.status.code()
    );
    assert!(
        stderr.contains("Verified OK"),
        "cosign exited 0 but stderr did not contain 'Verified OK':\nstderr={stderr}"
    );

    // ========================================================================
    // Negative assertions — a signature is only meaningful if verification
    // REJECTS tampered / mismatched inputs. Reuse the one real signed bundle
    // (sign is the expensive network step) and assert BOTH attestrum's own
    // verifier AND cosign reject each forgery. Closes the integrity-audit
    // 2026-05-29 "no crypto-rejection test exists anywhere" gap (item b/2).
    // ========================================================================

    // N1 — flipped signature byte: decode the DSSE signature, flip one bit,
    // re-encode. Valid base64, cryptographically wrong signature.
    let flipped = tmpdir.join("bundle.flipped-sig.sigstore.json");
    {
        let mut v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&bundle_path).expect("read bundle"))
                .expect("parse bundle json");
        let sig_b64 = v["dsseEnvelope"]["signatures"][0]["sig"]
            .as_str()
            .expect("dsseEnvelope.signatures[0].sig present")
            .to_string();
        let mut sig = base64::engine::general_purpose::STANDARD
            .decode(sig_b64.as_bytes())
            .expect("decode DSSE sig");
        sig[0] ^= 0x01;
        v["dsseEnvelope"]["signatures"][0]["sig"] =
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&sig));
        std::fs::write(&flipped, serde_json::to_vec(&v).expect("serialize"))
            .expect("write flipped");
    }
    let err = attest_verify_result(&flipped, &manifest_path, &identity_pattern, &issuer_pattern)
        .expect_err("flipped-signature bundle must be rejected by attestrum verify");
    assert!(
        matches!(err, AttestrumAttestError::SigstoreVerify(_)),
        "flipped signature should fail crypto verify, got {err:?}"
    );
    assert_cosign_rejects(
        &flipped,
        &manifest_path,
        &identity_pattern,
        &signed.oidc_issuer,
    );

    // N2 — wrong manifest: the valid bundle verified against different bytes.
    // sigstore recomputes sha256(manifest) and asserts == signed subject digest.
    let wrong_manifest = tmpdir.join("wrong-manifest.parquet");
    std::fs::write(&wrong_manifest, b"these are not the signed manifest bytes")
        .expect("write wrong manifest");
    let err = attest_verify_result(
        &bundle_path,
        &wrong_manifest,
        &identity_pattern,
        &issuer_pattern,
    )
    .expect_err("wrong-manifest must be rejected by attestrum verify");
    assert!(
        matches!(err, AttestrumAttestError::SigstoreVerify(_)),
        "wrong manifest should fail subject-digest check, got {err:?}"
    );
    assert_cosign_rejects(
        &bundle_path,
        &wrong_manifest,
        &identity_pattern,
        &signed.oidc_issuer,
    );

    // N3 — identity-regex mismatch: well-formed, crypto-valid bundle, but the
    // operator policy demands an identity this cert does not carry. Rejected
    // BEFORE the network crypto step (attestrum verify.rs step 3).
    let no_match = "^this-identity-will-never-match$";
    let err = attest_verify_result(&bundle_path, &manifest_path, no_match, &issuer_pattern)
        .expect_err("identity-regex mismatch must be rejected");
    assert!(
        matches!(err, AttestrumAttestError::IdentityPolicyMismatch { .. }),
        "identity mismatch should be IdentityPolicyMismatch, got {err:?}"
    );
    assert_cosign_rejects(&bundle_path, &manifest_path, no_match, &signed.oidc_issuer);

    // N4 — truncated-on-disk bundle: first half of the bytes. Parsing fails
    // before any crypto; verification must still reject (not panic).
    let truncated = tmpdir.join("bundle.truncated.sigstore.json");
    let full = std::fs::read(&bundle_path).expect("read bundle for truncation");
    std::fs::write(&truncated, &full[..full.len() / 2]).expect("write truncated");
    assert!(
        attest_verify_result(
            &truncated,
            &manifest_path,
            &identity_pattern,
            &issuer_pattern
        )
        .is_err(),
        "truncated bundle must be rejected by attestrum verify"
    );
    assert_cosign_rejects(
        &truncated,
        &manifest_path,
        &identity_pattern,
        &signed.oidc_issuer,
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

/// Require the OIDC token — PANIC (not skip) when absent. The test is
/// `#[ignore]`d so default `cargo test` skips it entirely; when it IS run (CI,
/// via `--include-ignored`), a missing token must FAIL loudly. A silent
/// skip-as-pass was a false trust signal (integrity-audit 2026-05-29 item b/1).
fn require_token() -> String {
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => panic!(
            "cosign_interop: SIGSTORE_ID_TOKEN unset/empty — this test must FAIL, not skip, when \
             run. It is #[ignore]'d (default `cargo test` skips it); CI runs it via \
             --include-ignored where .github/workflows/cosign-interop.yml exports the token from \
             the GHA OIDC exchange."
        ),
    }
}

/// Require `cosign` on PATH — PANIC (not skip) when absent. CI installs it via
/// sigstore/cosign-installer@v3; its absence in a run is a real failure.
fn require_cosign() {
    let ok = Command::new("cosign")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(
        ok,
        "cosign_interop: `cosign` not found on PATH — must FAIL, not skip (CI installs it via \
         sigstore/cosign-installer@v3)."
    );
}

/// Run attestrum's own verifier over a bundle/manifest pair (online).
fn attest_verify_result(
    bundle: &Path,
    manifest: &Path,
    identity_regex: &str,
    issuer_regex: &str,
) -> Result<VerifiedAttestation, AttestrumAttestError> {
    attest_verify(VerifyRequest {
        bundle_path: bundle,
        manifest_path: manifest,
        identity_regex,
        issuer_regex,
        offline: false,
        expected_predicate_type: None,
    })
}

/// Assert `cosign verify-blob-attestation` REJECTS the inputs (non-zero exit).
/// Mirrors the positive invocation's flag shape.
fn assert_cosign_rejects(bundle: &Path, manifest: &Path, identity_regex: &str, issuer: &str) {
    let out = Command::new("cosign")
        .arg("verify-blob-attestation")
        .arg("--new-bundle-format")
        .arg("--bundle")
        .arg(bundle)
        .arg("--certificate-identity-regexp")
        .arg(identity_regex)
        .arg("--certificate-oidc-issuer")
        .arg(issuer)
        .arg(manifest)
        .output()
        .expect("spawn cosign verify-blob-attestation");
    assert!(
        !out.status.success(),
        "cosign UNEXPECTEDLY accepted a tampered/mismatched input \
         (bundle={bundle:?}, manifest={manifest:?}, identity_regex={identity_regex}):\n\
         stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn hex_64(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}
