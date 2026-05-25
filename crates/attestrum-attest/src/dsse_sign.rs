//! DSSE-wrapped Sigstore Bundle v0.3 sign flow.
//!
//! Thin Attestrum-side wrapper over the fork's
//! [`sigstore::bundle::sign::blocking::SigningSession::sign_dsse`] method
//! (added at fork commit `e551bf9` on the
//! `attestrum/email-optional-for-workload-identity-tokens` branch via
//! workspace `[patch.crates-io]`). The fork-side method builds the DSSE
//! Pre-Authentication Encoding (PAE) bytes, signs them with the ephemeral
//! ECDSA-P256 key from the session's Fulcio cert, posts a Rekor v1
//! `dsse@0.0.1` transparency-log entry, and assembles a Sigstore Bundle
//! v0.3 with `Content::DsseEnvelope`. This module wraps the per-payload
//! invocation pattern and exposes a [`compute_pae`] helper for unit
//! tests that pin the PAE byte shape against the DSSE spec.
//!
//! Designed for extraction. Session 6 (out-of-band, async) lifts the
//! fork-side `sign_dsse` method into upstream sigstore-rs as a PR; once
//! that lands and the workspace's `[patch.crates-io]` block can be
//! dropped, this module's surface stays as-is — the only change is the
//! import path to the upstream `pub` method.
//!
//! See `docs/diagrams/sprint-4/sign-flow.md` for the end-to-end sequence
//! diagram (`source_of_truth: diagram` until Session 4's cosign-interop
//! CI flip).

use sigstore::bundle::sign::SigningContext;
use sigstore::bundle::Bundle;
use sigstore::oauth::IdentityToken;

use crate::AttestrumAttestError;

/// Sign `payload` as a DSSE-wrapped attestation and return a Sigstore
/// Bundle v0.3 with `Content::DsseEnvelope` plus a Rekor v1 `dsse@0.0.1`
/// transparency-log entry.
///
/// `payload_type` is canonically `"application/vnd.in-toto+json"` for
/// in-toto v1 Statements. `payload` is the raw canonical-JSON bytes —
/// the DSSE envelope's `payload` field carries base64(payload), but the
/// PAE that gets signed uses the raw bytes (see [`compute_pae`]).
///
/// **Network required.** The session construction hits Fulcio for cert
/// issuance; the fork-side `sign_dsse` hits Rekor for the tlog entry.
/// Callers must gate on `--offline` BEFORE invoking this function.
pub fn sign_dsse(
    ctx: &SigningContext,
    id_token: IdentityToken,
    payload_type: &str,
    payload: &[u8],
) -> Result<Bundle, AttestrumAttestError> {
    let session = ctx
        .blocking_signer(id_token)
        .map_err(|e| AttestrumAttestError::SigstoreSession(e.to_string()))?;
    session
        .sign_dsse(payload_type, payload)
        .map_err(|e| AttestrumAttestError::DsseSign(e.to_string()))
}

/// Compute the DSSE Pre-Authentication Encoding (PAE) bytes for the
/// given `payload_type` and `payload`.
///
/// PAE per <https://github.com/secure-systems-lab/dsse/blob/master/protocol.md>:
///
/// ```text
/// "DSSEv1 " || LEN(TYPE) || " " || TYPE || " " || LEN(PAYLOAD) || " " || PAYLOAD
/// ```
///
/// where `LEN(x)` is the **UTF-8 byte length** of `x` (= `x.len()` for
/// Rust `&str`/`&[u8]`), NOT the character count, and `PAYLOAD` is the
/// **raw payload bytes** (not base64-wrapped — the base64 only appears
/// in the on-disk DSSE envelope's `payload` field).
///
/// This helper is byte-identical to the verifier-side
/// `sigstore::bundle::verify::models::compute_pae` (`pub(crate)` in
/// sigstore-rs); we re-implement here so unit tests can pin the byte
/// shape against the DSSE spec without a fork dep.
pub fn compute_pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let header = format!(
        "DSSEv1 {} {} {} ",
        payload_type.len(),
        payload_type,
        payload.len()
    );
    let mut out = header.into_bytes();
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec example from <https://github.com/secure-systems-lab/dsse/blob/master/protocol.md>
    /// section "Protocol" worked example. payload_type = "http://example.com/HelloWorld"
    /// (29 bytes), payload = b"hello world" (11 bytes).
    #[test]
    fn compute_pae_matches_spec_hello_world_vector() {
        let payload_type = "http://example.com/HelloWorld";
        let payload = b"hello world";
        let pae = compute_pae(payload_type, payload);
        let expected: &[u8] = b"DSSEv1 29 http://example.com/HelloWorld 11 hello world";
        assert_eq!(pae, expected);
    }

    /// Empty payload — length prefix must be `0`, trailing space then
    /// zero bytes.
    #[test]
    fn compute_pae_handles_empty_payload() {
        let payload_type = "application/vnd.in-toto+json";
        let payload: &[u8] = b"";
        let pae = compute_pae(payload_type, payload);
        let expected: &[u8] = b"DSSEv1 28 application/vnd.in-toto+json 0 ";
        assert_eq!(pae, expected);
    }

    /// Multi-byte UTF-8 in payload_type — the rocket emoji is 4 UTF-8
    /// bytes, so LEN(TYPE) must be `4` not `1`. Regression guard against
    /// future refactors swapping `.len()` for `.chars().count()`.
    #[test]
    fn compute_pae_uses_utf8_byte_length_for_payload_type() {
        let payload_type = "\u{1F680}"; // 🚀 — 4 UTF-8 bytes
        let payload = b"x";
        let pae = compute_pae(payload_type, payload);
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"DSSEv1 4 ");
        expected.extend_from_slice(payload_type.as_bytes());
        expected.extend_from_slice(b" 1 x");
        assert_eq!(pae, expected);
    }

    /// Payload contains arbitrary binary bytes (including a NUL and
    /// high-bit bytes). PAE must concatenate the raw bytes verbatim —
    /// no escaping, no encoding.
    #[test]
    fn compute_pae_preserves_arbitrary_binary_payload_bytes() {
        let payload_type = "application/octet-stream";
        let payload: &[u8] = &[0x00, 0xFF, 0x7F, 0x80, b'\n', b'"'];
        let pae = compute_pae(payload_type, payload);
        let mut expected: Vec<u8> = Vec::new();
        expected.extend_from_slice(b"DSSEv1 24 application/octet-stream 6 ");
        expected.extend_from_slice(payload);
        assert_eq!(pae, expected);
    }
}
