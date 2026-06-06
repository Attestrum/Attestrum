//! Commit 3 — `cosign verify-blob-attestation --new-bundle-format` third-party
//! interop for **signed inclusion proofs** (`inclusion-proof/v0.3`).
//!
//! The §2.5 third-party-validator gate for `crates/attestrum-prove`'s emitted
//! inclusion-proof attestation: a third party with **zero Attestrum installed**
//! must be able to verify a signed inclusion proof using `cosign` alone. This is
//! the prove-side analogue of `crates/attestrum-attest/tests/cosign_interop.rs`
//! (which covers the training-corpus predicate over the manifest).
//!
//! **The non-obvious correctness point**: the cosign blob for an inclusion proof
//! is the **passage file**, NOT the manifest. The in-toto Statement's subject is
//! the matched leaf (`crates/attestrum-prove/src/lib.rs::entry_to_subject` —
//! `subject.digest.sha256 = leaf.sha256 = SHA-256(passage bytes)`), so cosign
//! recomputes `sha256(passage_file)` and matches it against the signed subject.
//! Verifying against the manifest would fail. The test therefore seals a **real**
//! one-passage corpus (so the leaf's hashes are over real file bytes) and proves
//! `ProofTarget::Document(passage_file)`.
//!
//! **Self-verify uses [`attestrum_attest::verify_statement`], not `verify`**:
//! `verify` hard-pins `expected_predicate_type = training-corpus`
//! (`verify.rs:249`) and would reject an inclusion proof at the predicate-type
//! gate. `verify_statement` is the predicate-agnostic core that gates on a
//! caller-supplied `expected_predicate_type` — here `inclusion-proof/v0.3`.
//!
//! Flow: seal a real 1-passage corpus → signed `prove(Document, …, sign:true)` →
//! self-verify online via `verify_statement` (also extracts the signing
//! identity/issuer) → `cosign verify-blob-attestation … <passage_file>` with
//! `--type …/inclusion-proof/v0.3` (positive) → four tamper negatives (flipped
//! signature, wrong blob, identity mismatch, truncated bundle), each asserting
//! BOTH attestrum's verifier AND cosign reject.
//!
//! **Fail-not-skip preconditions** (mirrors cosign_interop): `SIGSTORE_ID_TOKEN`
//! must be set (GHA OIDC exchange) and `cosign` must be on PATH (cosign-installer)
//! — missing either → PANIC (the test FAILS, not skips). `#[ignore]`d by default;
//! `.github/workflows/prove-sign-interop.yml` runs it via `--include-ignored`.

use std::path::Path;
use std::process::Command;

use base64::Engine;

use attestrum_attest::{
    verify_statement, AttestrumAttestError, DigestMap, InTotoStatement, Subject, VerifiedStatement,
    VerifyRequest, INCLUSION_PROOF_PREDICATE_TYPE, TRAINING_CORPUS_PREDICATE_TYPE,
};
use attestrum_cas::CasStore;
use attestrum_core::{BuildContext, Modality};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, ContentSource, CorpusEntry};
use attestrum_prove::{prove, ManifestSource, ProofKind, ProofTarget, ProveOpts};

/// A realistic, fixed passage. Real text (not random bytes) so the sealed
/// leaf and the on-disk blob are a faithful stand-in for a curated WikiText
/// passage; fixed so the corpus seals deterministically.
const PASSAGE: &[u8] = b"Valkyria of the Battlefield 3, commonly referred to as \
Valkyria Chronicles III outside Japan, is a tactical role-playing game developed \
by Sega and Media.Vision for the PlayStation Portable.";

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN + cosign on PATH + network; runs in .github/workflows/prove-sign-interop.yml only"]
fn prove_sign_interop() {
    let oidc_token = require_token();
    require_cosign();

    let tmpdir = std::env::temp_dir().join(format!(
        "attestrum-prove-sign-interop-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmpdir).expect("tmpdir create");

    // The passage on disk — this is the cosign blob. Its raw bytes drive the
    // sealed leaf's BLAKE3 + SHA-256, so the proof subject digest equals
    // sha256(this file).
    let passage_path = tmpdir.join("passage-01.txt");
    std::fs::write(&passage_path, PASSAGE).expect("write passage");

    // Seal a REAL one-passage corpus through the production pipeline. The
    // manifest leaf's document_id/sha256 are computed over PASSAGE's bytes, so
    // an exact Document prove against the same bytes hits leaf_index 0.
    let workspace = tmpdir.join("workspace");
    let ctx = BuildContext::new(workspace.clone(), 1_700_000_000);
    let cas = CasStore::new(workspace.join("cas")).expect("CasStore::new under tmpdir");
    let out_dir = workspace.join("out");
    let entries = vec![CorpusEntry {
        source_uri: "file:///corpus/passage-01.txt".into(),
        content: ContentSource::Bytes(PASSAGE.to_vec()),
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
    }];
    let build_output =
        build_corpus(&ctx, &cas, &entries, &out_dir).expect("build_corpus over the passage");
    let manifest_path = build_output.manifest_path.clone();
    assert_eq!(build_output.leaf_count, 1, "single-passage corpus");

    // A corpus-bundle fixture so the proof's `corpus.attestation_digest` is
    // populated (mirrors the real mint, which binds to the published signed
    // corpus bundle). Unsigned in-toto Statement is sufficient — the digest is
    // over the Statement's canonical JSON, not a signature.
    let corpus_bundle = tmpdir.join("corpus.bundle.json");
    let corpus_statement = InTotoStatement::new(
        TRAINING_CORPUS_PREDICATE_TYPE,
        vec![Subject {
            name: "corpus://wikitext-103-sealed/interop".to_string(),
            digest: DigestMap {
                blake3: attestrum_core::hex::encode_32(&build_output.merkle_root),
                sha256: "00".repeat(32),
            },
        }],
        serde_json::json!({
            "merkleRoot": attestrum_core::hex::encode_32(&build_output.merkle_root),
        }),
    );
    std::fs::write(
        &corpus_bundle,
        corpus_statement
            .canonical_json()
            .expect("canonical")
            .as_bytes(),
    )
    .expect("write corpus fixture");

    // Signed prove against Sigstore public-good (Fulcio + Rekor v2 + TUF).
    let prove_ws = tmpdir.join("prove-ws");
    let opts = ProveOpts {
        sign: true,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: Some(oidc_token),
        workspace: Some(prove_ws.clone()),
        corpus_bundle_path: Some(corpus_bundle),
        cas_root: None, // exact Document match needs no CAS re-fingerprint
        no_index: false,
    };
    let artifact = prove(
        ProofTarget::Document(passage_path.clone()),
        ManifestSource::Local(manifest_path),
        &opts,
    )
    .expect("signed Document prove against real Fulcio + Rekor");

    assert_eq!(
        artifact.kind,
        ProofKind::Inclusion,
        "exact passage match is an inclusion proof"
    );
    assert_eq!(artifact.confidence, 1.0, "exact match → confidence 1.00");
    assert_eq!(
        artifact.statement.predicate_type, INCLUSION_PROOF_PREDICATE_TYPE,
        "predicate type is inclusion-proof/v0.3"
    );
    let bundle_path = artifact
        .bundle_path
        .clone()
        .expect("opts.sign=true must populate bundle_path");
    assert_eq!(
        bundle_path.file_name().and_then(|s| s.to_str()),
        Some("inclusion-proof.sigstore.json"),
        "signed inclusion proof lands at <ws>/prove/inclusion-proof.sigstore.json"
    );

    // Positive self-verify (online) — also extracts the signing identity +
    // issuer from the freshly-signed bundle (prove() signs internally, so unlike
    // the attest cosign_interop test we have no SignedBundle return). Permissive
    // identity/issuer regexes (`.+`) accept any GHA workflow SAN; the
    // predicate-type gate is pinned to inclusion-proof/v0.3.
    let vs = verify_statement(VerifyRequest {
        bundle_path: &bundle_path,
        manifest_path: &passage_path, // the blob IS the passage, not the manifest
        identity_regex: ".+",
        issuer_regex: ".+",
        offline: false,
        expected_predicate_type: Some(INCLUSION_PROOF_PREDICATE_TYPE),
    })
    .expect("verify_statement self-verify (inclusion-proof) sanity gate");

    // Anchored, escaped policy patterns derived from the extracted identity —
    // the strict shape an operator would enforce (mirrors cosign_interop).
    let identity_pattern = format!("^{}$", regex::escape(&vs.identity));
    let issuer_pattern = format!("^{}$", regex::escape(&vs.oidc_issuer));

    // Shell out to cosign — the third-party gate. `--type` pins the inclusion
    // proof predicate URI (recent cosign rejects a non-default predicate type
    // when `--type` is absent); the blob is the passage file.
    let cosign = Command::new("cosign")
        .arg("verify-blob-attestation")
        .arg("--new-bundle-format")
        .arg("--type")
        .arg(INCLUSION_PROOF_PREDICATE_TYPE)
        .arg("--bundle")
        .arg(&bundle_path)
        .arg("--certificate-identity-regexp")
        .arg(&identity_pattern)
        .arg("--certificate-oidc-issuer")
        .arg(&vs.oidc_issuer)
        .arg(&passage_path)
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
        "cosign exited 0 but stderr lacked 'Verified OK':\nstderr={stderr}"
    );

    // ========================================================================
    // Negatives — a signature is only meaningful if verification REJECTS
    // tampered / mismatched inputs. Reuse the one real signed bundle (sign is
    // the expensive network step); assert BOTH attestrum's verifier AND cosign
    // reject each forgery. Mirrors cosign_interop N1-N4, blob = passage file.
    // ========================================================================

    // N1 — flipped signature byte: valid base64, cryptographically wrong sig.
    let flipped = tmpdir.join("inclusion.flipped-sig.sigstore.json");
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
    let err = verify_inclusion(&flipped, &passage_path, &identity_pattern, &issuer_pattern)
        .expect_err("flipped-signature bundle must be rejected by attestrum verify");
    assert!(
        matches!(err, AttestrumAttestError::SigstoreVerify(_)),
        "flipped signature should fail crypto verify, got {err:?}"
    );
    assert_cosign_rejects(&flipped, &passage_path, &identity_pattern, &vs.oidc_issuer);

    // N2 — wrong blob: the valid bundle verified against different bytes.
    // sigstore recomputes sha256(blob) and asserts == signed subject digest.
    let wrong_blob = tmpdir.join("wrong-passage.txt");
    std::fs::write(&wrong_blob, b"these are not the sealed passage bytes")
        .expect("write wrong blob");
    let err = verify_inclusion(
        &bundle_path,
        &wrong_blob,
        &identity_pattern,
        &issuer_pattern,
    )
    .expect_err("wrong-blob must be rejected by attestrum verify");
    assert!(
        matches!(err, AttestrumAttestError::SigstoreVerify(_)),
        "wrong blob should fail subject-digest check, got {err:?}"
    );
    assert_cosign_rejects(
        &bundle_path,
        &wrong_blob,
        &identity_pattern,
        &vs.oidc_issuer,
    );

    // N3 — identity-regex mismatch: crypto-valid bundle, operator policy demands
    // an identity this cert does not carry. Rejected BEFORE the network crypto.
    let no_match = "^this-identity-will-never-match$";
    let err = verify_inclusion(&bundle_path, &passage_path, no_match, &issuer_pattern)
        .expect_err("identity-regex mismatch must be rejected");
    assert!(
        matches!(err, AttestrumAttestError::IdentityPolicyMismatch { .. }),
        "identity mismatch should be IdentityPolicyMismatch, got {err:?}"
    );
    assert_cosign_rejects(&bundle_path, &passage_path, no_match, &vs.oidc_issuer);

    // N4 — truncated-on-disk bundle: first half of the bytes. Parsing fails
    // before any crypto; verification must reject (not panic).
    let truncated = tmpdir.join("inclusion.truncated.sigstore.json");
    let full = std::fs::read(&bundle_path).expect("read bundle for truncation");
    std::fs::write(&truncated, &full[..full.len() / 2]).expect("write truncated");
    assert!(
        verify_inclusion(
            &truncated,
            &passage_path,
            &identity_pattern,
            &issuer_pattern
        )
        .is_err(),
        "truncated bundle must be rejected by attestrum verify"
    );
    assert_cosign_rejects(
        &truncated,
        &passage_path,
        &identity_pattern,
        &vs.oidc_issuer,
    );

    let _ = std::fs::remove_dir_all(&tmpdir);
}

/// Require the OIDC token — PANIC (not skip) when absent. The test is
/// `#[ignore]`d so default `cargo test` skips it; when CI runs it via
/// `--include-ignored`, a missing token must FAIL loudly (a silent skip-as-pass
/// is a false trust signal).
fn require_token() -> String {
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => panic!(
            "prove_sign_interop: SIGSTORE_ID_TOKEN unset/empty — this test must FAIL, not skip. \
             It is #[ignore]'d (default `cargo test` skips it); CI runs it via --include-ignored \
             where .github/workflows/prove-sign-interop.yml exports the token from the GHA OIDC \
             exchange."
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
        "prove_sign_interop: `cosign` not found on PATH — must FAIL, not skip (CI installs it via \
         sigstore/cosign-installer@v3)."
    );
}

/// Run attestrum's predicate-agnostic verifier over an inclusion-proof bundle
/// + blob pair (online), pinned to the inclusion-proof predicate type.
fn verify_inclusion(
    bundle: &Path,
    blob: &Path,
    identity_regex: &str,
    issuer_regex: &str,
) -> Result<VerifiedStatement, AttestrumAttestError> {
    verify_statement(VerifyRequest {
        bundle_path: bundle,
        manifest_path: blob,
        identity_regex,
        issuer_regex,
        offline: false,
        expected_predicate_type: Some(INCLUSION_PROOF_PREDICATE_TYPE),
    })
}

/// Assert `cosign verify-blob-attestation` REJECTS the inputs (non-zero exit).
/// Mirrors the positive invocation's flag shape, including the `--type` pin.
fn assert_cosign_rejects(bundle: &Path, blob: &Path, identity_regex: &str, issuer: &str) {
    let out = Command::new("cosign")
        .arg("verify-blob-attestation")
        .arg("--new-bundle-format")
        .arg("--type")
        .arg(INCLUSION_PROOF_PREDICATE_TYPE)
        .arg("--bundle")
        .arg(bundle)
        .arg("--certificate-identity-regexp")
        .arg(identity_regex)
        .arg("--certificate-oidc-issuer")
        .arg(issuer)
        .arg(blob)
        .output()
        .expect("spawn cosign verify-blob-attestation");
    assert!(
        !out.status.success(),
        "cosign UNEXPECTEDLY accepted a tampered/mismatched input \
         (bundle={bundle:?}, blob={blob:?}, identity_regex={identity_regex}):\n\
         stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
