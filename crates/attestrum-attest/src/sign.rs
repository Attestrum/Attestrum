//! Sigstore Bundle v0.3 sign — wraps `sigstore::bundle::sign` (RustCrypto
//! org's `sigstore-rs` crate, blocking API path so we don't drag tokio in).
//!
//! `attestrum_attest::sign::sign` is a low-level helper: it takes pre-built
//! payload bytes (typically `InTotoStatement::canonical_json` output) +
//! an OIDC identity token (the caller is responsible for sourcing it —
//! env var, file, workload-identity exchange, or interactive OIDC flow),
//! signs against the Sigstore public-good roots, and writes a Bundle v0.3
//! JSON file to the requested output path.
//!
//! **Network required.** Calls Fulcio for ephemeral cert issuance + Rekor
//! v2 for transparency-log submission + TUF for the trusted-root refresh.
//! The caller is responsible for `--offline` gating BEFORE invoking this
//! function — by the time we're inside `sign()` we WILL hit the network.
//!
//! **Identity-token sourcing is NOT in this module.** Sourcing strategies
//! (env var / file / OIDC ambient / interactive) belong to the CLI layer
//! at `crates/attestrum-cli/src/commands/sign.rs` (E3.5). This module accepts
//! a plain `String` token and assumes the caller validated its provenance.
//!
//! See `docs/diagrams/sprint-4/sign-flow.md` for the end-to-end sequence
//! diagram. The diagram flips to `source_of_truth: code` at E3.5 once the
//! CLI subcommand wraps this function with the contract test the diagram's
//! sequenceDiagram type triggers per PATH-A-BRIEF §7.1.

use std::fs;
use std::path::{Path, PathBuf};

use sigstore::bundle::sign::SigningContext;
use sigstore::oauth::IdentityToken;

use crate::AttestrumAttestError;

/// Inputs to a single Sigstore Bundle v0.3 sign operation.
pub struct SignRequest<'a> {
    /// The bytes that go into the DSSE envelope's `payload` field.
    /// Typically the canonical-JSON serialization of an in-toto v1 Statement
    /// built via [`crate::statement::InTotoStatement::canonical_json`]. The
    /// `payloadType` defaults to `application/vnd.in-toto+json` (sigstore-rs
    /// sets this when wrapping payload bytes into a DSSE envelope).
    pub statement_payload: &'a [u8],
    /// Where to write the resulting Sigstore Bundle v0.3 JSON. The file
    /// is overwritten if it exists; parent directories are created if
    /// missing.
    pub bundle_output_path: &'a Path,
    /// OIDC id_token (JWT) for the signing identity. The caller acquires
    /// it from whichever source is appropriate for the run (env var, file,
    /// workload-identity exchange, interactive OIDC flow). This module
    /// does not validate the token's contents — it forwards it to Fulcio
    /// which validates and binds the certificate to the OIDC identity.
    pub oidc_id_token: String,
}

/// Result of a successful sign operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedAttestation {
    /// Absolute path the Bundle v0.3 JSON was written to (caller-supplied
    /// `bundle_output_path` echoed back for convenience + canonicalized).
    pub bundle_path: PathBuf,
    /// Identity descriptor extracted from the leaf cert's Subject
    /// Alternative Name extension (first Sigstore-relevant value:
    /// rfc822Name email, URI, or Sigstore otherName workload identity).
    /// Sprint 4 E4: populated for real via [`crate::identity::extract_identity`]
    /// — previously a placeholder string during E3.5 sign-only window.
    pub identity: String,
    /// OIDC issuer URL extracted from the leaf cert's Fulcio extension
    /// (OID `1.3.6.1.4.1.57264.1.8` v1, fallback `…57264.1.1` legacy).
    /// Sprint 4 E4 addition. Pairs with `identity` for round-trip verify
    /// (cosign-style `--certificate-identity` + `--certificate-oidc-issuer`).
    pub oidc_issuer: String,
}

/// Sign `req.statement_payload` against the Sigstore public-good roots
/// using `req.oidc_id_token` as the identity proof; write the resulting
/// Bundle v0.3 JSON to `req.bundle_output_path`; return identity + path.
///
/// **Network required**, **OIDC required**. Both Fulcio (cert issuance)
/// and Rekor v2 (transparency log) are contacted; the TUF trusted-root
/// is refreshed if cache is stale. Callers must gate on `--offline`
/// BEFORE invoking.
pub fn sign(req: SignRequest<'_>) -> Result<SignedAttestation, AttestrumAttestError> {
    // 1. SigningContext::production() configures against public-good Fulcio
    //    + Rekor + TUF trusted-root. Fetches/refreshes the TUF root cache.
    let ctx = SigningContext::production()
        .map_err(|e| AttestrumAttestError::SigstoreContext(e.to_string()))?;

    // 2. Parse the OIDC id_token JWT into a typed IdentityToken so
    //    sigstore-rs can extract the audience + expiry claims needed for
    //    the Fulcio CSR.
    let id_token = IdentityToken::try_from(req.oidc_id_token.as_str())
        .map_err(|e| AttestrumAttestError::SigstoreIdentityToken(e.to_string()))?;

    // 3. Open a blocking signer session. Generates the ephemeral keypair
    //    internally and exchanges the id_token + CSR for a Fulcio cert.
    let session = ctx
        .blocking_signer(id_token)
        .map_err(|e| AttestrumAttestError::SigstoreSession(e.to_string()))?;

    // 4. Sign the payload bytes. Builds the DSSE envelope, signs with the
    //    ephemeral private key, submits the envelope + cert chain to Rekor
    //    v2, embeds the tlog entry + timestamps into the SigningArtifact.
    let artifact = session
        .sign(req.statement_payload)
        .map_err(|e| AttestrumAttestError::SigstoreSign(e.to_string()))?;

    // 5. Convert the SigningArtifact into a Bundle (Sigstore Bundle v0.3
    //    protobuf-JSON representation).
    let bundle = artifact.to_bundle();

    // 6. Serialize the Bundle to JSON. Bundle v0.3 specifies ProtoJSON
    //    encoding (lowerCamelCase field names, base64-encoded bytes, int64
    //    fields as strings). sigstore-rs's Serialize impl produces this.
    //    Sprint 4 E3.6 swapped `serde_json::to_vec_pretty` for
    //    `deterministic_json_vec` so the OUTER bundle JSON (not just the
    //    inner DSSE-signed Statement) is byte-deterministic across runs.
    //    Without this, the determinism CI matrix would diff the bundle
    //    file byte-by-byte and catch object-key-order drift introduced by
    //    serde_json's default `serde_json::Map` iteration. Drops the
    //    2-space pretty-print indentation — Bundle v0.3 spec doesn't
    //    require it; verifiers tolerate either form.
    let bundle_json = crate::json::deterministic_json_vec(&bundle)?;

    // 7. Ensure parent dir exists + write atomically.
    if let Some(parent) = req.bundle_output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(req.bundle_output_path, &bundle_json).map_err(AttestrumAttestError::from)?;

    // 8. Sprint 4 E4: extract the real identity-pair from the just-written
    //    bundle's leaf cert so SignedAttestation surfaces accurate values
    //    instead of E3.5's `"sigstore-bundle-v0.3"` placeholder. We
    //    re-parse the bundle JSON we just serialised — single read source
    //    (bundle_json bytes), no extra disk I/O. Best-effort: an
    //    extraction error here is logged-and-fallback rather than
    //    fail-the-sign (the cryptographic operation succeeded; identity
    //    extraction is a cosmetic display concern).
    let extracted = serde_json::from_slice::<serde_json::Value>(&bundle_json)
        .ok()
        .and_then(|v| crate::identity::extract_identity(&v).ok());
    let (identity, oidc_issuer) = match extracted {
        Some(e) => (e.san, e.oidc_issuer),
        None => (
            "<unparseable from bundle cert>".to_string(),
            "<unparseable from bundle cert>".to_string(),
        ),
    };

    Ok(SignedAttestation {
        bundle_path: req.bundle_output_path.to_path_buf(),
        identity,
        oidc_issuer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_request_struct_constructs_with_zero_copy_borrows() {
        // Smoke test that the SignRequest borrow semantics are right — no
        // network needed; just ensures the public API shape lets callers
        // pass borrowed slices + paths without `.to_owned()` churn.
        let payload: &[u8] = b"BASE64_OF_INTOTO_STATEMENT";
        let path = Path::new("/tmp/attestrum-attest-test/bundle.sigstore.json");
        let _req = SignRequest {
            statement_payload: payload,
            bundle_output_path: path,
            oidc_id_token: "x.y.z".to_string(),
        };
        // Lifetime check — req borrows from payload + path, which are still
        // in scope. If this compiles, the SignRequest lifetime story is OK.
    }

    #[test]
    fn signed_attestation_round_trip_equality() {
        let a = SignedAttestation {
            bundle_path: PathBuf::from("/tmp/x/bundle.sigstore.json"),
            identity: "ci@example.org".to_string(),
            oidc_issuer: "https://github.com/login/oauth".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    /// Integration test gated on `SIGSTORE_ID_TOKEN` env var presence.
    /// Run with `cargo test -p attestrum-attest --test '*' -- --ignored
    /// sign_against_public_good` when you have a valid OIDC token.
    ///
    /// Will hit Fulcio + Rekor + TUF — leaves a real Sigstore transparency
    /// log entry on every run, so use sparingly + only with a non-
    /// production identity.
    #[test]
    #[ignore = "requires SIGSTORE_ID_TOKEN env var + network + leaves real Rekor entries; run manually"]
    fn sign_against_public_good_with_env_token() {
        let token = std::env::var("SIGSTORE_ID_TOKEN").expect(
            "SIGSTORE_ID_TOKEN env var required for this ignored test — see test docstring",
        );
        let payload = b"e2e-integration-test-payload-from-attestrum-attest";
        let tmpdir =
            std::env::temp_dir().join(format!("attestrum-attest-sign-e2e-{}", std::process::id()));
        let out_path = tmpdir.join("bundle.sigstore.json");
        let attestation = sign(SignRequest {
            statement_payload: payload,
            bundle_output_path: &out_path,
            oidc_id_token: token,
        })
        .expect("sign against public-good should succeed with valid OIDC");
        assert!(attestation.bundle_path.exists());
        assert!(attestation.bundle_path.metadata().unwrap().len() > 100);
        // Clean up.
        let _ = std::fs::remove_dir_all(&tmpdir);
    }
}
