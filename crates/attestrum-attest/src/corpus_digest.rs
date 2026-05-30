//! `attestation_digest_of_bundle` — the canonical corpus attestation digest.
//!
//! [`crate::CorpusRef::attestation_digest`] is, by definition, the BLAKE3 +
//! SHA-256 digest of the corpus in-toto Statement's `canonical_json()` bytes —
//! i.e. the bytes that become the DSSE envelope's `payload` when the Statement
//! is signed. This module computes exactly that, identically whether the input
//! is a **signed Sigstore Bundle v0.3** (extract `dsseEnvelope.payload`, base64)
//! or a **raw Statement JSON** (parse directly).
//!
//! **Why this exists (Commit 1, the determinism bugfix).** `attestrum-prove`
//! previously digested the *whole bundle file* (`stream_hash_path`). For a
//! signed bundle that file carries 16 non-deterministic fields (cert chain,
//! Rekor tlog entries, timestamps, the DSSE signature, the ephemeral public
//! key — see [`crate::canonicalize::STRIP_PATHS`]), so the digest differed on
//! every signing of the same Statement — a determinism bug (CLAUDE.md §7) that
//! also contradicted [`crate::CorpusRef`]'s own doc-comment. Hashing the
//! canonical Statement bytes instead is deterministic and makes signed and
//! unsigned corpora produce the same `attestationDigest`.
//!
//! **Both branches re-emit `canonical_json()` before hashing** rather than
//! trusting the embedded payload bytes: a corpus signed by stock `cosign`
//! (rather than `attestrum sign`) need not key-sort its DSSE payload, so
//! re-emitting the canonical form is what keeps the binding chain walkable for
//! any in-toto-conformant signer (the CLAUDE.md §12 vendor-neutrality promise).

use std::path::Path;

use base64::Engine as _;
use sha2::Digest as _;

use crate::predicate::DigestMap;
use crate::statement::InTotoStatement;
use crate::AttestrumAttestError;

/// Compute the canonical `attestationDigest` of the corpus attestation at
/// `path` — the BLAKE3 + SHA-256 of the corpus Statement's `canonical_json()`
/// bytes, identical for a signed Sigstore Bundle v0.3 and a raw Statement.
///
/// Returns [`AttestrumAttestError::Json`] if the file is neither a Statement
/// nor a DSSE-bundle, and [`AttestrumAttestError::Io`] if it cannot be read.
pub fn attestation_digest_of_bundle(path: &Path) -> Result<DigestMap, AttestrumAttestError> {
    let bytes = std::fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let statement = statement_from_value(value)?;
    let canonical = statement.canonical_json()?;
    Ok(digest_bytes(canonical.as_bytes()))
}

/// Resolve a parsed JSON value to the corpus [`InTotoStatement`], handling both
/// the signed-bundle and raw-Statement shapes. The returned Statement is
/// re-serialized canonically by the caller — the embedded encoding is never
/// trusted to be canonical.
fn statement_from_value(value: serde_json::Value) -> Result<InTotoStatement, AttestrumAttestError> {
    if let Some(payload_b64) = value
        .get("dsseEnvelope")
        .and_then(|d| d.get("payload"))
        .and_then(serde_json::Value::as_str)
    {
        let payload = base64::engine::general_purpose::STANDARD
            .decode(payload_b64)
            .map_err(|e| {
                AttestrumAttestError::SigstoreVerify(format!(
                    "corpus bundle dsseEnvelope.payload base64 decode failed: {e}"
                ))
            })?;
        let statement: InTotoStatement = serde_json::from_slice(&payload)?;
        statement.validate()?;
        Ok(statement)
    } else {
        let statement: InTotoStatement = serde_json::from_value(value)?;
        statement.validate()?;
        Ok(statement)
    }
}

/// BLAKE3 + SHA-256 of `bytes`, hex-encoded into a [`DigestMap`]. Mirrors the
/// pair `attestrum_cas::stream_hash` produces, computed in-memory because the
/// canonical Statement bytes are already materialized.
fn digest_bytes(bytes: &[u8]) -> DigestMap {
    let blake3 = blake3::hash(bytes);
    let sha256: [u8; 32] = sha2::Sha256::digest(bytes).into();
    DigestMap {
        blake3: attestrum_core::hex::encode_32(blake3.as_bytes()),
        sha256: attestrum_core::hex::encode_32(&sha256),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::Subject;
    use crate::TRAINING_CORPUS_PREDICATE_TYPE;

    fn sample_statement() -> InTotoStatement {
        let subject = Subject {
            name: "corpus://acme/pretraining".to_string(),
            digest: DigestMap {
                blake3: "ab".repeat(32),
                sha256: "cd".repeat(32),
            },
        };
        InTotoStatement::new(
            TRAINING_CORPUS_PREDICATE_TYPE,
            vec![subject],
            serde_json::json!({"merkleRoot": "ef".repeat(32), "note": "sample"}),
        )
    }

    #[test]
    fn raw_statement_value_digests_to_canonical() {
        let stmt = sample_statement();
        let canonical = stmt.canonical_json().unwrap();
        let expected = digest_bytes(canonical.as_bytes());

        let value = serde_json::to_value(&stmt).unwrap();
        let resolved = statement_from_value(value).unwrap();
        let got = digest_bytes(resolved.canonical_json().unwrap().as_bytes());

        assert_eq!(got, expected);
    }

    #[test]
    fn signed_bundle_payload_digests_equal_to_raw() {
        let stmt = sample_statement();
        let canonical = stmt.canonical_json().unwrap();
        let expected = digest_bytes(canonical.as_bytes());

        // A signed bundle wraps the canonical Statement bytes as the DSSE
        // payload (base64). The digest must match the raw-Statement digest.
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(canonical.as_bytes());
        let bundle = serde_json::json!({ "dsseEnvelope": { "payload": payload_b64 } });
        let resolved = statement_from_value(bundle).unwrap();
        let got = digest_bytes(resolved.canonical_json().unwrap().as_bytes());

        assert_eq!(
            got, expected,
            "signed and unsigned corpora must produce the same attestationDigest"
        );
    }

    #[test]
    fn non_canonical_payload_still_digests_equal() {
        // A non-Attestrum signer (e.g. stock cosign) need not key-sort its DSSE
        // payload. The helper re-emits canonical_json(), so a key-reordered
        // payload that parses to the same Statement must yield the same digest.
        let stmt = sample_statement();
        let expected = digest_bytes(stmt.canonical_json().unwrap().as_bytes());

        // Hand-build a payload with keys deliberately NOT in canonical (sorted)
        // order, but semantically identical to `stmt`.
        let non_canonical = format!(
            r#"{{"subject":[{{"name":"{}","digest":{{"sha256":"{}","blake3":"{}"}}}}],"predicate":{{"note":"sample","merkleRoot":"{}"}},"predicateType":"{}","_type":"https://in-toto.io/Statement/v1"}}"#,
            "corpus://acme/pretraining",
            "cd".repeat(32),
            "ab".repeat(32),
            "ef".repeat(32),
            TRAINING_CORPUS_PREDICATE_TYPE,
        );
        let payload_b64 =
            base64::engine::general_purpose::STANDARD.encode(non_canonical.as_bytes());
        let bundle = serde_json::json!({ "dsseEnvelope": { "payload": payload_b64 } });
        let resolved = statement_from_value(bundle).unwrap();
        let got = digest_bytes(resolved.canonical_json().unwrap().as_bytes());

        assert_eq!(
            got, expected,
            "a non-canonical (cosign-style) payload must still link via canonical re-emit"
        );
    }

    #[test]
    fn reads_from_disk() {
        let stmt = sample_statement();
        let expected = digest_bytes(stmt.canonical_json().unwrap().as_bytes());

        let path = std::env::temp_dir().join("attestrum-attest-corpus-digest-reads-from-disk.json");
        std::fs::write(&path, stmt.canonical_json().unwrap().as_bytes()).unwrap();
        let got = attestation_digest_of_bundle(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(got, expected);
    }

    #[test]
    fn rejects_non_statement_file() {
        let path = std::env::temp_dir().join("attestrum-attest-corpus-digest-rejects-junk.json");
        std::fs::write(&path, br#"{"placeholder":"not a statement"}"#).unwrap();
        let got = attestation_digest_of_bundle(&path);
        let _ = std::fs::remove_file(&path);

        assert!(
            got.is_err(),
            "a file that is neither a Statement nor a DSSE bundle must be rejected"
        );
    }
}
