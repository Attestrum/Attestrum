//! in-toto Statement v1 wrapper.
//!
//! Spec: `https://in-toto.io/Statement/v1` (defined at
//! `github.com/in-toto/attestation/blob/main/spec/v1/statement.md`).
//! Shape: `{ _type, subject[], predicateType, predicate }`.

use serde::{Deserialize, Serialize};

use crate::predicate::Subject;
use crate::AttestrumAttestError;

/// in-toto Statement v1 `_type` URI (spec-mandated const).
pub const IN_TOTO_STATEMENT_V1_TYPE_URI: &str = "https://in-toto.io/Statement/v1";

/// in-toto v1 Statement wrapper.
///
/// The `predicate` field is `serde_json::Value` rather than a generic-typed
/// predicate so a single Statement type can carry any of the three Attestrum
/// predicate payload shapes (training-corpus, inclusion-proof,
/// non-inclusion-proof) discriminated by `predicate_type` URI. Callers
/// typically build the predicate value via `serde_json::to_value(&pred)?`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InTotoStatement {
    /// in-toto spec field name is `_type`, not `type` (Python-style sentinel).
    /// Always [`IN_TOTO_STATEMENT_V1_TYPE_URI`] at v1; validated via
    /// [`Self::validate`].
    #[serde(rename = "_type")]
    pub type_uri: String,
    pub subject: Vec<Subject>,
    pub predicate_type: String,
    pub predicate: serde_json::Value,
}

impl InTotoStatement {
    /// Construct a Statement with `_type` defaulted to
    /// [`IN_TOTO_STATEMENT_V1_TYPE_URI`].
    pub fn new(
        predicate_type: impl Into<String>,
        subject: Vec<Subject>,
        predicate: serde_json::Value,
    ) -> Self {
        Self {
            type_uri: IN_TOTO_STATEMENT_V1_TYPE_URI.to_string(),
            subject,
            predicate_type: predicate_type.into(),
            predicate,
        }
    }

    /// Validates the `_type` const field per the in-toto v1 spec.
    pub fn validate(&self) -> Result<(), AttestrumAttestError> {
        if self.type_uri != IN_TOTO_STATEMENT_V1_TYPE_URI {
            return Err(AttestrumAttestError::InTotoTypeMismatch {
                expected: IN_TOTO_STATEMENT_V1_TYPE_URI,
                actual: self.type_uri.clone(),
            });
        }
        Ok(())
    }

    /// Serialize to canonical JSON with recursively sorted object keys.
    ///
    /// This is the bytes that go through `base64()` into the DSSE
    /// envelope's `payload` field. Determinism across runs + platforms
    /// requires sorted keys so the base64-encoded payload is byte-
    /// identical given identical predicate content.
    ///
    /// Routes through [`crate::json::deterministic_json`] — the single
    /// sanctioned sort-then-serialize path. Sprint 4 E3.6 collapsed three
    /// duplicate hand-rolled recursive-sort fns into that one helper.
    pub fn canonical_json(&self) -> Result<String, AttestrumAttestError> {
        Ok(crate::json::deterministic_json(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::DigestMap;
    use serde_json::json;

    fn sample_subject() -> Subject {
        Subject {
            name: "manifest.parquet".to_string(),
            digest: DigestMap {
                blake3: "a".repeat(64),
                sha256: "b".repeat(64),
            },
        }
    }

    #[test]
    fn new_defaults_type_uri_to_in_toto_v1() {
        let stmt = InTotoStatement::new(
            "https://attestrum.com/attestation/training-corpus/v0.1",
            vec![sample_subject()],
            json!({}),
        );
        assert_eq!(stmt.type_uri, IN_TOTO_STATEMENT_V1_TYPE_URI);
        stmt.validate().unwrap();
    }

    #[test]
    fn validate_rejects_wrong_type_uri() {
        let bad = InTotoStatement {
            type_uri: "https://in-toto.io/Statement/v0.1".to_string(),
            subject: vec![sample_subject()],
            predicate_type: "x".to_string(),
            predicate: json!({}),
        };
        match bad.validate() {
            Err(AttestrumAttestError::InTotoTypeMismatch { expected, actual }) => {
                assert_eq!(expected, IN_TOTO_STATEMENT_V1_TYPE_URI);
                assert_eq!(actual, "https://in-toto.io/Statement/v0.1");
            }
            other => panic!("expected InTotoTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_via_serde_json_with_underscore_type_field() {
        let stmt = InTotoStatement::new(
            "https://attestrum.com/attestation/training-corpus/v0.1",
            vec![sample_subject()],
            json!({"merkleRoot": "ff", "rowCount": 1000}),
        );
        let json = serde_json::to_string(&stmt).unwrap();
        // _type field (with underscore) must round-trip exactly
        assert!(json.contains("\"_type\":\"https://in-toto.io/Statement/v1\""));
        assert!(json.contains(
            "\"predicateType\":\"https://attestrum.com/attestation/training-corpus/v0.1\""
        ));
        let back: InTotoStatement = serde_json::from_str(&json).unwrap();
        assert_eq!(stmt, back);
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let stmt = InTotoStatement::new(
            "x",
            vec![sample_subject()],
            json!({"z": 1, "a": {"y": 2, "b": 3}}),
        );
        let canonical = stmt.canonical_json().unwrap();
        // Top-level keys sorted: _type, predicate, predicateType, subject
        let idx_type = canonical.find("\"_type\"").unwrap();
        let idx_predicate = canonical.find("\"predicate\"").unwrap();
        let idx_predicate_type = canonical.find("\"predicateType\"").unwrap();
        let idx_subject = canonical.find("\"subject\"").unwrap();
        assert!(idx_type < idx_predicate, "_type < predicate");
        assert!(
            idx_predicate < idx_predicate_type,
            "predicate < predicateType"
        );
        assert!(idx_predicate_type < idx_subject, "predicateType < subject");
        // Nested predicate object keys also sorted: a < z
        let inner_a = canonical.find("\"a\"").unwrap();
        let inner_z = canonical.find("\"z\"").unwrap();
        assert!(inner_a < inner_z);
        // Inner-inner: b < y
        let inner_b = canonical.find("\"b\"").unwrap();
        let inner_y = canonical.find("\"y\"").unwrap();
        assert!(inner_b < inner_y);
    }

    #[test]
    fn canonical_json_is_deterministic_across_repeated_calls() {
        let stmt = InTotoStatement::new(
            "x",
            vec![sample_subject()],
            json!({"z": [3, 1, 2], "a": {"nested": true}}),
        );
        let a = stmt.canonical_json().unwrap();
        let b = stmt.canonical_json().unwrap();
        assert_eq!(a, b);
        // Arrays preserve order (subjects are ordered per in-toto spec)
        assert!(a.contains("[3,1,2]"));
    }
}
