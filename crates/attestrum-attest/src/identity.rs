//! Extract the (identity, oidc_issuer) tuple from a Sigstore Bundle v0.3's
//! leaf certificate. Used by both the sign side (to fill
//! [`crate::sign::SignedAttestation`]'s identity fields with real values
//! instead of the E3.5 placeholder string) and the verify side (to display
//! the resolved identity + double-check it matches the operator-supplied
//! regex flags).
//!
//! **What's extracted**:
//! - `identity`: the first Sigstore-relevant value from the leaf cert's
//!   Subject Alternative Name extension. SAN entries are inspected in
//!   order; the first matching `rfc822Name` (email), `uniformResourceIdentifier`
//!   (URI), or `otherName` with OID `1.3.6.1.4.1.57264.1.7` (the Sigstore
//!   custom-SAN OID for workload identities) wins.
//! - `oidc_issuer`: the OIDC issuer URL from the Fulcio custom extension.
//!   Tries OID `1.3.6.1.4.1.57264.1.8` first (Fulcio v1 form, DER UTF8String)
//!   then falls back to OID `1.3.6.1.4.1.57264.1.1` (legacy form, raw bytes
//!   that are the ASCII URL).
//!
//! **What's NOT done here**:
//! - Regex matching of identity / issuer against operator-supplied patterns
//!   — that's [`crate::verify`]'s job.
//! - Cert chain validation — that's sigstore-rs's `Verifier::verify` job.
//! - SAN/issuer normalization (case-folding, trailing-slash handling) — we
//!   return the raw cert-extension string so the verify-side regex can
//!   anchor on the exact form Fulcio issued.

use base64::Engine;
use serde_json::Value;
use x509_cert::der::Decode;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::Certificate;

use crate::AttestrumAttestError;

/// Fulcio OIDC-issuer extension OIDs.
///
/// `1.3.6.1.4.1.57264.1.8` is the v1 form (DER UTF8String value).
/// `1.3.6.1.4.1.57264.1.1` is the legacy form (raw bytes = ASCII URL).
///
/// We try v1 first because it's what current Fulcio (post-2023) emits for
/// new certs; legacy is the fallback for any older bundles still in the
/// wild. See <https://github.com/sigstore/fulcio/blob/main/docs/oid-info.md>.
const FULCIO_OIDC_ISSUER_V1_OID: &str = "1.3.6.1.4.1.57264.1.8";
const FULCIO_OIDC_ISSUER_LEGACY_OID: &str = "1.3.6.1.4.1.57264.1.1";

/// The Sigstore custom-SAN OID for workload identities (e.g., GitHub
/// Actions `https://github.com/owner/repo/.github/workflows/...@refs/heads/main`).
/// Per <https://github.com/sigstore/fulcio/blob/main/docs/oid-info.md> §
/// 1.3.6.1.4.1.57264.1.7.
const SIGSTORE_OTHERNAME_OID: &str = "1.3.6.1.4.1.57264.1.7";

/// Extracted identity-pair from a Sigstore Bundle's leaf certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedIdentity {
    /// First Sigstore-relevant value from the leaf cert's Subject Alternative
    /// Name extension. For human identities: an email address (e.g.,
    /// `alice@example.org`). For workload identities: a URI (e.g., the GHA
    /// workflow trigger URL).
    pub san: String,
    /// OIDC issuer URL from the Fulcio cert extension (e.g.,
    /// `https://github.com/login/oauth` or `https://accounts.google.com`).
    pub oidc_issuer: String,
}

/// Extract the identity pair from a Sigstore Bundle v0.3 JSON value.
///
/// Accepts both Bundle v0.3 keyless form (`verificationMaterial.certificate`)
/// and legacy chain form (`verificationMaterial.x509CertificateChain.certificates[0]`).
/// Returns [`AttestrumAttestError::IdentityExtractionFailed`] on any failure
/// (no cert present, malformed DER, no SAN, no OIDC-issuer extension).
pub fn extract_identity(bundle: &Value) -> Result<ExtractedIdentity, AttestrumAttestError> {
    let cert_der = locate_leaf_cert_der(bundle)?;
    let cert = Certificate::from_der(&cert_der)
        .map_err(|e| AttestrumAttestError::IdentityExtractionFailed(format!("cert parse: {e}")))?;

    let san = extract_san(&cert)?;
    let oidc_issuer = extract_oidc_issuer(&cert)?;
    Ok(ExtractedIdentity { san, oidc_issuer })
}

/// Navigate the bundle JSON and return the leaf-cert DER bytes.
fn locate_leaf_cert_der(bundle: &Value) -> Result<Vec<u8>, AttestrumAttestError> {
    let vm = bundle.get("verificationMaterial").ok_or_else(|| {
        AttestrumAttestError::IdentityExtractionFailed(
            "bundle.verificationMaterial missing".to_string(),
        )
    })?;

    // Bundle v0.3 keyless form: verificationMaterial.certificate.rawBytes
    // (base64-encoded DER).
    if let Some(raw_b64) = vm
        .get("certificate")
        .and_then(|c| c.get("rawBytes"))
        .and_then(Value::as_str)
    {
        return decode_b64_der(raw_b64);
    }

    // Legacy chain form: verificationMaterial.x509CertificateChain.certificates[0].rawBytes.
    if let Some(raw_b64) = vm
        .get("x509CertificateChain")
        .and_then(|c| c.get("certificates"))
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("rawBytes"))
        .and_then(Value::as_str)
    {
        return decode_b64_der(raw_b64);
    }

    Err(AttestrumAttestError::IdentityExtractionFailed(
        "bundle.verificationMaterial.certificate.rawBytes (or legacy chain form) missing"
            .to_string(),
    ))
}

fn decode_b64_der(b64: &str) -> Result<Vec<u8>, AttestrumAttestError> {
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| {
            AttestrumAttestError::IdentityExtractionFailed(format!(
                "base64 decode cert rawBytes: {e}"
            ))
        })
}

fn extract_san(cert: &Certificate) -> Result<String, AttestrumAttestError> {
    let san_extension: (bool, SubjectAltName) = cert
        .tbs_certificate
        .get::<SubjectAltName>()
        .map_err(|e| {
            AttestrumAttestError::IdentityExtractionFailed(format!("SAN extension parse: {e}"))
        })?
        .ok_or_else(|| {
            AttestrumAttestError::IdentityExtractionFailed(
                "leaf cert has no SubjectAltName extension".to_string(),
            )
        })?;

    let san = san_extension.1;
    for name in san.0.iter() {
        match name {
            GeneralName::Rfc822Name(s) => return Ok(s.to_string()),
            GeneralName::UniformResourceIdentifier(s) => return Ok(s.to_string()),
            GeneralName::OtherName(other) => {
                if other.type_id.to_string() == SIGSTORE_OTHERNAME_OID {
                    // Sigstore otherName value is a DER UTF8String inside
                    // the OctetString wrapper. Decode the inner UTF8String.
                    if let Ok(s) = std::str::from_utf8(other.value.value()) {
                        return Ok(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    Err(AttestrumAttestError::IdentityExtractionFailed(
        "no rfc822Name / URI / Sigstore-otherName in SAN".to_string(),
    ))
}

fn extract_oidc_issuer(cert: &Certificate) -> Result<String, AttestrumAttestError> {
    let extensions = cert.tbs_certificate.extensions.as_ref().ok_or_else(|| {
        AttestrumAttestError::IdentityExtractionFailed("leaf cert has no extensions".to_string())
    })?;

    // Try v1 OID first (DER UTF8String value).
    for ext in extensions {
        if ext.extn_id.to_string() == FULCIO_OIDC_ISSUER_V1_OID {
            // The extn_value is an OctetString whose contents are a DER
            // UTF8String for the v1 form.
            let inner = ext.extn_value.as_bytes();
            // Try DER-decode as Utf8StringRef.
            if let Ok(s) = x509_cert::der::asn1::Utf8StringRef::from_der(inner) {
                return Ok(s.as_str().to_string());
            }
            // Fall through and try other interpretations if the DER decode
            // failed — some Fulcio versions may have shipped bare bytes
            // even under the v1 OID; treat the bytes as UTF-8 directly.
            if let Ok(s) = std::str::from_utf8(inner) {
                return Ok(s.to_string());
            }
        }
    }

    // Fall back to legacy OID (raw bytes = ASCII URL).
    for ext in extensions {
        if ext.extn_id.to_string() == FULCIO_OIDC_ISSUER_LEGACY_OID {
            let bytes = ext.extn_value.as_bytes();
            if let Ok(s) = std::str::from_utf8(bytes) {
                return Ok(s.to_string());
            }
        }
    }

    Err(AttestrumAttestError::IdentityExtractionFailed(
        "leaf cert has no Fulcio OIDC-issuer extension (tried OID 57264.1.8 v1 + 57264.1.1 legacy)"
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn locate_leaf_cert_der_keyless_form() {
        // base64 of a 3-byte garbage placeholder: "AAEC" → [0x00, 0x01, 0x02].
        let bundle = json!({
            "verificationMaterial": {
                "certificate": { "rawBytes": "AAEC" }
            }
        });
        let bytes = locate_leaf_cert_der(&bundle).unwrap();
        assert_eq!(bytes, vec![0x00, 0x01, 0x02]);
    }

    #[test]
    fn locate_leaf_cert_der_legacy_chain_form() {
        let bundle = json!({
            "verificationMaterial": {
                "x509CertificateChain": {
                    "certificates": [
                        { "rawBytes": "AAEC" }
                    ]
                }
            }
        });
        let bytes = locate_leaf_cert_der(&bundle).unwrap();
        assert_eq!(bytes, vec![0x00, 0x01, 0x02]);
    }

    #[test]
    fn locate_leaf_cert_der_missing_verification_material_errors() {
        let bundle = json!({});
        let err = locate_leaf_cert_der(&bundle).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("verificationMaterial"),
            "error should mention verificationMaterial: {msg}"
        );
    }

    #[test]
    fn locate_leaf_cert_der_no_cert_field_errors() {
        let bundle = json!({
            "verificationMaterial": {
                "messageSignature": {}
            }
        });
        let err = locate_leaf_cert_der(&bundle).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("certificate.rawBytes") || msg.contains("legacy chain"),
            "error should mention missing cert path: {msg}"
        );
    }

    #[test]
    fn extract_identity_propagates_malformed_der_as_cert_parse_error() {
        // Valid base64, but the decoded bytes are NOT a valid X.509 cert.
        let bundle = json!({
            "verificationMaterial": {
                "certificate": { "rawBytes": "AAEC" }
            }
        });
        let err = extract_identity(&bundle).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cert parse"),
            "error should mention cert parse failure: {msg}"
        );
    }

    #[test]
    fn decode_b64_der_invalid_base64_returns_error() {
        let err = decode_b64_der("not-valid-base64!!!").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("base64"), "error should mention base64: {msg}");
    }
}
