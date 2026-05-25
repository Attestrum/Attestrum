//! Sprint 4 E4.5 — `cosign verify-blob-attestation --new-bundle-format`
//! third-party interop.
//!
//! Closes Sprint 4 acceptance per `PATH-A-BRIEF.md` §1.5 + Part 6 Sprint 4:
//! a third party with zero Attestrum installed must be able to verify our
//! bundles using `cosign` alone. This test signs a real bundle against the
//! Sigstore public-good roots, self-verifies via [`attestrum_attest::verify`]
//! as a sanity gate, then shells out to `cosign` and asserts the same
//! bundle round-trips.
//!
//! **Skip-with-log preconditions** (mirrors the
//! `sign_against_public_good_with_env_token` convention at
//! `crates/attestrum-attest/src/sign.rs:197`):
//!
//! - `SIGSTORE_ID_TOKEN` env var must be set (sourced from the GHA OIDC
//!   exchange in `.github/workflows/cosign-interop.yml`).
//! - `cosign` binary must be on PATH (installed via the workflow's
//!   `sigstore/cosign-installer@v3` step).
//!
//! Missing either precondition → log to stderr and return early (the test
//! passes trivially). Network failures, sign failures, self-verify
//! failures, and cosign mismatches all fail the test.
//!
//! `#[ignore]`d by default — `cargo test --workspace` lists but does not
//! execute it. The dedicated workflow runs the ignored test via
//! `cargo test --workspace --test cosign_interop -- --include-ignored cosign_interop`.
//! The first GREEN CI run is gated on the founder's first public push to
//! `github.com/Attestrum/Attestrum`.

use std::process::Command;

use attestrum_attest::{
    sign as attest_sign, verify as attest_verify, DeterminismFields, DigestMap, InTotoStatement,
    LicensingPosture, ManifestRef, RulesetMode, SignRequest, SignalCoverage, Subject,
    TrainingCorpusPredicate, VerifyRequest, TRAINING_CORPUS_PREDICATE_TYPE,
};
use attestrum_cas::{stream_hash, CasStore};
use attestrum_core::BuildContext;
use attestrum_pipeline::build_corpus;

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN + cosign on PATH + network; runs in .github/workflows/cosign-interop.yml only"]
fn cosign_interop() {
    let Some(oidc_token) = env_token() else {
        return;
    };
    if !cosign_on_path() {
        eprintln!(
            "cosign_interop: `cosign` not found on PATH — skipping. Install via \
             sigstore/cosign-installer@v3 in the CI workflow."
        );
        return;
    }

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
    })
    .expect("attestrum_attest::verify self-verify sanity gate");

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

    let _ = std::fs::remove_dir_all(&tmpdir);
}

fn env_token() -> Option<String> {
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(t) if !t.is_empty() => Some(t),
        _ => {
            eprintln!(
                "cosign_interop: SIGSTORE_ID_TOKEN not set — skipping. This test only runs \
                 in the dedicated CI workflow where the GHA OIDC exchange exports the token."
            );
            None
        }
    }
}

fn cosign_on_path() -> bool {
    Command::new("cosign")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn hex_64(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}
