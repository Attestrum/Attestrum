//! Sigstore Bundle v0.3 verify — wraps `sigstore::bundle::verify::blocking::Verifier`
//! (the synchronous wrapper around the async core, so we don't drag tokio
//! into call sites).
//!
//! Mirror of [`crate::sign::sign`]: low-level entry point for the verify
//! side of the Sigstore round-trip. Takes a bundle file path + a manifest
//! file path + an identity regex policy + an offline flag, returns a
//! [`VerifiedAttestation`] carrying the extracted identity pair, the parsed
//! in-toto Statement, the deserialised [`TrainingCorpusPredicate`], and
//! the Rekor tlog entry's integratedTime + logIndex.
//!
//! **Network unless `--offline`**. Calls Sigstore's TUF (refresh trusted
//! root) + Rekor (online inclusion-proof check, skipped under `offline`).
//! Caller (the CLI layer) is responsible for surfacing `--offline` to the
//! operator + mapping any TUF-refresh failure to either Exit 3 (offline
//! violation) or Exit 5 (network error) depending on the flag state.
//!
//! **Light-weight Exit 8 path**: after sigstore-rs's cryptographic verify
//! succeeds, we attempt to deserialise the in-toto Statement's predicate
//! as a [`TrainingCorpusPredicate`]. If deserialisation fails the bundle
//! is well-formed and crypto-valid but the predicate doesn't satisfy the
//! v0.3 schema — surface as [`AttestrumAttestError::PredicateValidationFailed`].
//! The Rust types ARE the schema (schemars-derived); no `jsonschema-rs`
//! dependency needed.

use std::fs::File;
use std::path::{Path, PathBuf};

use base64::Engine;
use regex::Regex;
use sigstore::bundle::verify::blocking::Verifier;
use sigstore::bundle::verify::policy::Identity;
use sigstore::bundle::Bundle;

use crate::identity::{extract_identity, ExtractedIdentity};
use crate::predicate::TrainingCorpusPredicate;
use crate::statement::InTotoStatement;
use crate::{AttestrumAttestError, TRAINING_CORPUS_PREDICATE_TYPE};

/// Inputs to a single Sigstore Bundle v0.3 verify operation.
pub struct VerifyRequest<'a> {
    /// Path to the Sigstore Bundle v0.3 JSON file.
    pub bundle_path: &'a Path,
    /// Path to the manifest file being attested (the in-toto subject).
    /// Verifier reads this file's bytes; sigstore-rs computes its SHA-256
    /// internally and asserts it matches the bundle's subject digest.
    pub manifest_path: &'a Path,
    /// Operator-supplied regex (anchored) matched against the extracted
    /// SAN. cosign-compatible `-regexp` semantics. Required; no default.
    pub identity_regex: &'a str,
    /// Operator-supplied regex (anchored) matched against the extracted
    /// Fulcio OIDC-issuer extension. Required; no default.
    pub issuer_regex: &'a str,
    /// Skip online Rekor inclusion-proof re-check; trust the bundle's
    /// embedded inclusion promise + signed entry timestamp. TUF root
    /// refresh still requires network unless the on-disk cache is fresh.
    pub offline: bool,
    /// The in-toto `predicateType` URI the caller expects. `None` defaults to
    /// [`TRAINING_CORPUS_PREDICATE_TYPE`], preserving every pre-binding caller.
    /// [`verify_statement`] gates on this value; [`verify`] always pins it to
    /// training-corpus (so its concrete-predicate deserialize stays coherent),
    /// while a binding/proof verifier passes e.g.
    /// [`crate::MODEL_BINDING_PREDICATE_TYPE`].
    pub expected_predicate_type: Option<&'a str>,
}

/// Result of a successful verify operation. Carries enough material for
/// the CLI's success print (identity, issuer, predicate type, Merkle
/// root via the parsed predicate, integrated time, log index) plus the
/// parsed in-toto Statement + predicate for callers wanting structured
/// access (e.g., `--print-predicate` rendering, future fingerprint flows).
#[derive(Debug, Clone)]
pub struct VerifiedAttestation {
    /// First Sigstore-relevant SAN value from the leaf cert (matched the
    /// `identity_regex` policy).
    pub identity: String,
    /// OIDC issuer URL from the Fulcio cert extension (matched the
    /// `issuer_regex` policy).
    pub oidc_issuer: String,
    /// in-toto `predicateType` URI (e.g.,
    /// `https://attestrum.com/attestation/training-corpus/v0.3`).
    pub predicate_type: String,
    /// Parsed in-toto v1 Statement (including the un-typed
    /// `predicate: serde_json::Value`).
    pub statement: InTotoStatement,
    /// Deserialised training-corpus predicate. Surfaces as the Exit 8
    /// validation gate during verify — if this fails to deserialise the
    /// verifier rejects the bundle with [`AttestrumAttestError::PredicateValidationFailed`].
    pub predicate: TrainingCorpusPredicate,
    /// Rekor tlog wall-clock integration time (Unix seconds).
    pub integrated_time: i64,
    /// Rekor global log ingest order.
    pub log_index: i64,
    /// Absolute path the bundle was read from (echoed back for the CLI
    /// success print).
    pub bundle_path: PathBuf,
}

/// Result of a successful crypto + identity verify, **before** any
/// concrete-predicate typing. Identical to [`VerifiedAttestation`] minus the
/// typed `predicate` field — the predicate stays untyped (read it off
/// [`InTotoStatement::predicate`], a `serde_json::Value`) so this path serves
/// the proof + binding predicate families, not just training-corpus.
///
/// Produced by [`verify_statement`]; the caller deserializes the predicate
/// into whatever concrete type the (already-checked) `predicateType` implies.
#[derive(Debug, Clone)]
pub struct VerifiedStatement {
    /// First Sigstore-relevant SAN value from the leaf cert (matched the
    /// `identity_regex` policy).
    pub identity: String,
    /// OIDC issuer URL from the Fulcio cert extension (matched the
    /// `issuer_regex` policy).
    pub oidc_issuer: String,
    /// in-toto `predicateType` URI — already checked against
    /// `req.expected_predicate_type`.
    pub predicate_type: String,
    /// Parsed in-toto v1 Statement (including the un-typed
    /// `predicate: serde_json::Value`).
    pub statement: InTotoStatement,
    /// Rekor tlog wall-clock integration time (Unix seconds).
    pub integrated_time: i64,
    /// Rekor global log ingest order.
    pub log_index: i64,
    /// Absolute path the bundle was read from.
    pub bundle_path: PathBuf,
}

/// Crypto + identity verify + `predicateType` gate, **without** deserializing a
/// concrete predicate. Returns a [`VerifiedStatement`] whose `predicate` stays
/// untyped, so this is the shared core that the proof + binding predicate
/// families verify through. [`verify`] is the typed training-corpus wrapper
/// over this.
///
/// The `predicateType` is checked against
/// `req.expected_predicate_type.unwrap_or(`[`TRAINING_CORPUS_PREDICATE_TYPE`]`)`.
///
/// **Network required for TUF refresh** unless the on-disk cache at
/// `~/.sigstore` is fresh. **Rekor inclusion-proof re-check is skipped
/// when `req.offline` is true** — the verifier still validates the bundle's
/// embedded signed inclusion proof + RFC3161 timestamp against the cached
/// trusted root.
pub fn verify_statement(req: VerifyRequest<'_>) -> Result<VerifiedStatement, AttestrumAttestError> {
    // 1. Read bundle bytes + parse JSON twice (once as serde_json::Value
    //    for identity extraction + tlog navigation; once as the sigstore
    //    proto Bundle for crypto verification). The two parses each
    //    surface a clear, distinct error path.
    let bundle_bytes = std::fs::read(req.bundle_path).map_err(AttestrumAttestError::Io)?;
    let bundle_value: serde_json::Value =
        serde_json::from_slice(&bundle_bytes).map_err(AttestrumAttestError::Json)?;
    let bundle_proto: Bundle =
        serde_json::from_slice(&bundle_bytes).map_err(AttestrumAttestError::Json)?;

    // 2. Extract identity-pair from the bundle's leaf cert (independent
    //    of sigstore-rs's verify; we need the raw values for the regex
    //    policy + the success-print display).
    let extracted: ExtractedIdentity = extract_identity(&bundle_value)?;

    // 3. Apply the operator-supplied identity / issuer regexes. Anchor
    //    both with `^...$` so the regex matches the whole string (mirrors
    //    cosign's `--certificate-identity-regexp` semantics). On
    //    mismatch: surface IdentityPolicyMismatch BEFORE invoking
    //    sigstore-rs — we don't want sigstore-rs to network-out trying
    //    to verify a bundle whose identity we've already rejected.
    let identity_re = compile_anchored_regex(req.identity_regex, "identity")?;
    let issuer_re = compile_anchored_regex(req.issuer_regex, "issuer")?;
    if !identity_re.is_match(&extracted.san) || !issuer_re.is_match(&extracted.oidc_issuer) {
        return Err(AttestrumAttestError::IdentityPolicyMismatch {
            extracted_identity: extracted.san.clone(),
            extracted_issuer: extracted.oidc_issuer.clone(),
            identity_regex: req.identity_regex.to_string(),
            issuer_regex: req.issuer_regex.to_string(),
        });
    }

    // 4. Construct the production verifier (TUF refresh against the
    //    public-good trusted root; cached at ~/.sigstore). This is the
    //    step that can fail for either offline-violation OR network
    //    reasons — the caller (CLI lifecycle) distinguishes the two via
    //    the `req.offline` flag state.
    let verifier =
        Verifier::production().map_err(|e| AttestrumAttestError::SigstoreContext(e.to_string()))?;

    // 5. Build the sigstore-rs Identity policy with the EXTRACTED literal
    //    values (we've already regex-matched). sigstore-rs's Identity is
    //    literal-string-only; feeding the extracted values is an exact
    //    match by construction so the policy gate is a no-op confirmation.
    let policy = Identity::new(&extracted.san, &extracted.oidc_issuer);

    // 6. Open the manifest file as a Read source + invoke the verifier.
    //    Verifier::verify computes SHA-256 of the manifest internally
    //    and asserts it matches the bundle's subject digest.
    let manifest_file = File::open(req.manifest_path).map_err(AttestrumAttestError::Io)?;
    verifier
        .verify(manifest_file, bundle_proto, &policy, req.offline)
        .map_err(|e| AttestrumAttestError::SigstoreVerify(format_error_chain(&e)))?;

    // 7. Extract the in-toto Statement payload (base64-encoded JSON in
    //    bundle.dsseEnvelope.payload) and parse it.
    let statement = extract_in_toto_statement(&bundle_value)?;

    // 8. Validate the predicate_type URI matches the caller's expectation
    //    (default training-corpus). A binding/proof verifier passes its own
    //    expected URI; any other URI is a bundle this call shouldn't accept.
    let expected = req
        .expected_predicate_type
        .unwrap_or(TRAINING_CORPUS_PREDICATE_TYPE);
    if statement.predicate_type != expected {
        return Err(AttestrumAttestError::PredicateValidationFailed(format!(
            "unexpected predicateType {:?}; this verify call expected {}",
            statement.predicate_type, expected
        )));
    }

    // 9. Pull integratedTime + logIndex from the Rekor tlog entry. Both
    //    are required fields in Bundle v0.3 for a valid Rekor entry; if
    //    missing the bundle was malformed in a way sigstore-rs should
    //    have caught — surface defensively.
    let (integrated_time, log_index) = extract_tlog_fields(&bundle_value)?;

    Ok(VerifiedStatement {
        identity: extracted.san,
        oidc_issuer: extracted.oidc_issuer,
        predicate_type: statement.predicate_type.clone(),
        statement,
        integrated_time,
        log_index,
        bundle_path: req.bundle_path.to_path_buf(),
    })
}

/// Verify `req.bundle_path` against `req.manifest_path` using the
/// operator's identity-regex policy, pinned to the training-corpus predicate.
/// Returns the [`VerifiedAttestation`] (with the concrete
/// [`TrainingCorpusPredicate`]) on success; one of the [`AttestrumAttestError`]
/// variants on failure.
///
/// A thin typed wrapper over [`verify_statement`]: it forces
/// `expected_predicate_type = `[`TRAINING_CORPUS_PREDICATE_TYPE`] (regardless of
/// what the caller passed, so the concrete-predicate deserialize is always
/// coherent), then runs the light-weight Exit-8 schema gate. Every pre-binding
/// caller keeps its exact behavior.
///
/// **Network required for TUF refresh** unless the on-disk cache at
/// `~/.sigstore` is fresh. **Rekor inclusion-proof re-check is skipped
/// when `req.offline` is true.**
pub fn verify(req: VerifyRequest<'_>) -> Result<VerifiedAttestation, AttestrumAttestError> {
    let vs = verify_statement(VerifyRequest {
        expected_predicate_type: Some(TRAINING_CORPUS_PREDICATE_TYPE),
        ..req
    })?;

    // Light-weight Exit-8 path: attempt-deserialise the predicate as a
    // TrainingCorpusPredicate. The Rust type IS the v0.3 schema
    // (schemars-derived from the same struct), so deserialise success ⇔
    // schema-validation success. No `jsonschema-rs` dep.
    let predicate: TrainingCorpusPredicate = serde_json::from_value(vs.statement.predicate.clone())
        .map_err(|e| {
            AttestrumAttestError::PredicateValidationFailed(format!(
                "predicate does not satisfy TrainingCorpusPredicate v0.3 schema: {e}"
            ))
        })?;

    Ok(VerifiedAttestation {
        identity: vs.identity,
        oidc_issuer: vs.oidc_issuer,
        predicate_type: vs.predicate_type,
        statement: vs.statement,
        predicate,
        integrated_time: vs.integrated_time,
        log_index: vs.log_index,
        bundle_path: vs.bundle_path,
    })
}

// ============================================================================
// Small helpers
// ============================================================================

fn compile_anchored_regex(pattern: &str, label: &str) -> Result<Regex, AttestrumAttestError> {
    let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
        pattern.to_string()
    } else if pattern.starts_with('^') {
        format!("{pattern}$")
    } else if pattern.ends_with('$') {
        format!("^{pattern}")
    } else {
        format!("^{pattern}$")
    };
    Regex::new(&anchored).map_err(|e| {
        AttestrumAttestError::IdentityExtractionFailed(format!(
            "{label} regex {pattern:?} did not compile (anchored as {anchored:?}): {e}"
        ))
    })
}

fn extract_in_toto_statement(
    bundle: &serde_json::Value,
) -> Result<InTotoStatement, AttestrumAttestError> {
    let payload_b64 = bundle
        .get("dsseEnvelope")
        .and_then(|d| d.get("payload"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AttestrumAttestError::SigstoreVerify(
                "bundle.dsseEnvelope.payload missing or not a string".to_string(),
            )
        })?;
    let payload_bytes = base64::engine::general_purpose::STANDARD
        .decode(payload_b64)
        .map_err(|e| {
            AttestrumAttestError::SigstoreVerify(format!(
                "bundle.dsseEnvelope.payload base64 decode failed: {e}"
            ))
        })?;
    let statement: InTotoStatement =
        serde_json::from_slice(&payload_bytes).map_err(AttestrumAttestError::Json)?;
    statement.validate()?;
    Ok(statement)
}

fn extract_tlog_fields(bundle: &serde_json::Value) -> Result<(i64, i64), AttestrumAttestError> {
    let entry = bundle
        .get("verificationMaterial")
        .and_then(|vm| vm.get("tlogEntries"))
        .and_then(serde_json::Value::as_array)
        .and_then(|arr| arr.first())
        .ok_or_else(|| {
            AttestrumAttestError::SigstoreVerify(
                "bundle.verificationMaterial.tlogEntries[0] missing".to_string(),
            )
        })?;
    // Bundle v0.3 protobuf-JSON encodes int64 as either a JSON number OR
    // a JSON string (per the ProtoJSON spec). Accept both.
    let integrated_time = entry
        .get("integratedTime")
        .and_then(parse_proto_int64)
        .ok_or_else(|| {
            AttestrumAttestError::SigstoreVerify(
                "bundle.verificationMaterial.tlogEntries[0].integratedTime missing".to_string(),
            )
        })?;
    let log_index = entry
        .get("logIndex")
        .and_then(parse_proto_int64)
        .ok_or_else(|| {
            AttestrumAttestError::SigstoreVerify(
                "bundle.verificationMaterial.tlogEntries[0].logIndex missing".to_string(),
            )
        })?;
    Ok((integrated_time, log_index))
}

fn parse_proto_int64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

/// Walk [`std::error::Error::source`] and format every level's Display
/// message on its own indented line. Used to preserve sigstore-rs's
/// `#[source]` chain when wrapping `SignatureVerificationError` /
/// `VerificationError` into [`AttestrumAttestError::SigstoreVerify`]'s
/// String payload — the top-level Display of sigstore-rs errors is
/// generic (`"signature verification failed"`) for any of cert-chain,
/// SCT, DSSE-math, Rekor-inclusion, or SAN-policy failures; the deeper
/// source frame names the actual subsystem and is what the cosign-interop
/// CI failure log needs to surface for diagnosis.
fn format_error_chain<E: std::error::Error + ?Sized>(err: &E) -> String {
    use std::fmt::Write as _;
    let mut out = format!("[0] {err}");
    let mut current = err.source();
    let mut depth = 1usize;
    while let Some(e) = current {
        let _ = write!(out, "\n  [{depth}] {e}");
        current = e.source();
        depth += 1;
    }
    out
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compile_anchored_regex_anchors_unanchored_patterns() {
        let re = compile_anchored_regex("alice@example.org", "identity").unwrap();
        assert!(re.is_match("alice@example.org"));
        assert!(!re.is_match("bob+alice@example.org")); // anchored at start
        assert!(!re.is_match("alice@example.org.evil")); // anchored at end
    }

    #[test]
    fn compile_anchored_regex_preserves_existing_anchors() {
        let re = compile_anchored_regex("^.+@example\\.org$", "identity").unwrap();
        assert!(re.is_match("alice@example.org"));
        assert!(re.is_match("bob@example.org"));
        assert!(!re.is_match("alice@example.org.evil"));
    }

    #[test]
    fn compile_anchored_regex_rejects_invalid_regex() {
        let err = compile_anchored_regex("[unclosed", "identity").unwrap_err();
        assert!(err.to_string().contains("did not compile"));
    }

    #[test]
    fn extract_in_toto_statement_decodes_payload() {
        // Build a valid in-toto statement, base64-encode, wrap as the
        // dsseEnvelope.payload field.
        let stmt = InTotoStatement::new(
            "https://attestrum.com/attestation/training-corpus/v0.3",
            vec![],
            json!({"merkleRoot": "ff"}),
        );
        let stmt_json = serde_json::to_vec(&stmt).unwrap();
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(&stmt_json);
        let bundle = json!({
            "dsseEnvelope": { "payload": payload_b64 }
        });
        let extracted = extract_in_toto_statement(&bundle).unwrap();
        assert_eq!(extracted.predicate_type, stmt.predicate_type);
        assert_eq!(extracted.predicate, stmt.predicate);
    }

    #[test]
    fn extract_in_toto_statement_missing_payload_errors() {
        let bundle = json!({ "dsseEnvelope": {} });
        let err = extract_in_toto_statement(&bundle).unwrap_err();
        assert!(err.to_string().contains("payload"));
    }

    #[test]
    fn extract_in_toto_statement_invalid_base64_errors() {
        let bundle = json!({ "dsseEnvelope": { "payload": "@@@not-base64@@@" } });
        let err = extract_in_toto_statement(&bundle).unwrap_err();
        assert!(err.to_string().contains("base64"));
    }

    #[test]
    fn extract_tlog_fields_accepts_number_form() {
        let bundle = json!({
            "verificationMaterial": {
                "tlogEntries": [{
                    "integratedTime": 1_748_109_600_i64,
                    "logIndex": 42_i64,
                }]
            }
        });
        let (it, li) = extract_tlog_fields(&bundle).unwrap();
        assert_eq!(it, 1_748_109_600);
        assert_eq!(li, 42);
    }

    #[test]
    fn extract_tlog_fields_accepts_string_form_per_protojson_int64() {
        let bundle = json!({
            "verificationMaterial": {
                "tlogEntries": [{
                    "integratedTime": "1748109600",
                    "logIndex": "42",
                }]
            }
        });
        let (it, li) = extract_tlog_fields(&bundle).unwrap();
        assert_eq!(it, 1_748_109_600);
        assert_eq!(li, 42);
    }

    #[test]
    fn extract_tlog_fields_missing_entries_errors() {
        let bundle = json!({"verificationMaterial": {}});
        let err = extract_tlog_fields(&bundle).unwrap_err();
        assert!(err.to_string().contains("tlogEntries"));
    }

    #[test]
    fn format_error_chain_walks_source_chain() {
        use std::error::Error;
        use std::fmt;

        #[derive(Debug)]
        struct ChainErr {
            msg: &'static str,
            src: Option<Box<dyn Error + 'static>>,
        }
        impl fmt::Display for ChainErr {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.msg)
            }
        }
        impl Error for ChainErr {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                self.src.as_deref()
            }
        }

        let leaf = ChainErr {
            msg: "x509 leaf parse failed",
            src: None,
        };
        let mid = ChainErr {
            msg: "cert chain validation failed",
            src: Some(Box::new(leaf)),
        };
        let top = ChainErr {
            msg: "signature verification failed",
            src: Some(Box::new(mid)),
        };

        let rendered = format_error_chain(&top);
        assert!(
            rendered.contains("[0] signature verification failed"),
            "missing top frame: {rendered}"
        );
        assert!(
            rendered.contains("[1] cert chain validation failed"),
            "missing middle frame: {rendered}"
        );
        assert!(
            rendered.contains("[2] x509 leaf parse failed"),
            "missing leaf frame: {rendered}"
        );
    }
}
