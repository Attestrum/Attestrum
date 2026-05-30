//! `attestrum-attest` — in-toto v1 Statement + Sigstore Bundle v0.3 predicate types.
//!
//! Sprint 4 commit E2 ships the three predicate Rust types
//! ([`predicate::TrainingCorpusPredicate`], [`predicate::InclusionProofPredicate`],
//! [`predicate::NonInclusionProofPredicate`]) plus the [`statement::InTotoStatement`]
//! wrapper plus the three [`pub const`] URI strings that identify each
//! predicate type. Constructor for `TrainingCorpusPredicate` is callable
//! today via direct struct construction; the two proof predicates have their
//! shapes locked here for v0.3 but their `pub fn build()` constructors land
//! at Sprint 5 alongside `attestrum prove`.
//!
//! **PROTECTED**: the three URI constants below are PROTECTED per CLAUDE.md §4.
//! Once published in this commit they are immutable at v0.3. Any change to
//! the URI strings or to the JSON-Schema shape of any predicate payload
//! requires a v0.4 URI bump + a published migration document + an in-toto
//! vetted-catalog re-submission. The shapes were originally locked in the
//! Annex-era v0.1/v0.2 cross-check (resolution doc retained local-only).
//!
//! See `docs/diagrams/sprint-4/predicate-three-types.md` for the class diagram
//! (now `source_of_truth: code` — this file is authoritative).

pub mod canonicalize;
mod corpus_digest;
pub mod dsse_sign;
pub mod identity;
pub mod json;
mod model_binding;
pub mod predicate;
pub mod sign;
pub mod statement;
pub mod verify;

use thiserror::Error;

pub use canonicalize::{canonicalize_for_compare, PathSegment, STRIP_PATHS, STRIP_SENTINEL};
pub use corpus_digest::attestation_digest_of_bundle;
pub use identity::{extract_identity, ExtractedIdentity};
pub use json::{deterministic_json, deterministic_json_vec, sort_keys};
pub use model_binding::{CorpusBindingRef, ModelBindingPredicate, ModelRef, TrainingMeta};
pub use predicate::{
    BoundaryCase, CorpusRef, DeterminismFields, DigestMap, InclusionProofPredicate, IsccEvidence,
    LicenseInventoryEntry, LicensingPosture, ManifestRef, MatchEvidence, MinHashEvidence, Neighbor,
    NonInclusionProofPredicate, PerceptualEvidence, PublicationIntent, RulesetMode, SignalCoverage,
    SortedAssertion, Subject, TrainingCorpusPredicate,
};
pub use sign::{sign, SignRequest, SignedAttestation};
pub use statement::{InTotoStatement, IN_TOTO_STATEMENT_V1_TYPE_URI};
pub use verify::{verify, VerifiedAttestation, VerifyRequest};

// ============================================================================
// PROTECTED — predicate type URIs (CLAUDE.md §4)
// ============================================================================

/// PROTECTED — `https://attestrum.com/attestation/training-corpus/v0.3`.
///
/// The `predicateType` URI string identifying a [`TrainingCorpusPredicate`]
/// payload inside an in-toto v1 Statement. **Immutable at v0.3**: changing
/// the string OR the schema shape requires a v0.4 URI bump per CLAUDE.md §4.
pub const TRAINING_CORPUS_PREDICATE_TYPE: &str =
    "https://attestrum.com/attestation/training-corpus/v0.3";

/// PROTECTED — `https://attestrum.com/attestation/inclusion-proof/v0.3`.
///
/// The `predicateType` URI string identifying an [`InclusionProofPredicate`]
/// payload. **Immutable at v0.3.**
pub const INCLUSION_PROOF_PREDICATE_TYPE: &str =
    "https://attestrum.com/attestation/inclusion-proof/v0.3";

/// PROTECTED — `https://attestrum.com/attestation/non-inclusion-proof/v0.3`.
///
/// The `predicateType` URI string identifying a [`NonInclusionProofPredicate`]
/// payload. **Immutable at v0.3.**
pub const NON_INCLUSION_PROOF_PREDICATE_TYPE: &str =
    "https://attestrum.com/attestation/non-inclusion-proof/v0.3";

/// All three predicate type URIs, in canonical order (training-corpus,
/// inclusion-proof, non-inclusion-proof). Used by `tests/api_surface.rs` for
/// the golden-file API-snapshot check.
pub const ALL_PREDICATE_TYPES: [&str; 3] = [
    TRAINING_CORPUS_PREDICATE_TYPE,
    INCLUSION_PROOF_PREDICATE_TYPE,
    NON_INCLUSION_PROOF_PREDICATE_TYPE,
];

/// PROTECTED — `https://attestrum.com/attestation/model-binding/v0.1`.
///
/// The `predicateType` URI string identifying a [`ModelBindingPredicate`]
/// payload (corpus-to-model binding). A **separate v0.1 generation** from the
/// frozen v0.3 corpus/proof family above: it is a standalone const and is
/// **deliberately NOT a member of [`ALL_PREDICATE_TYPES`]** (D2-A), which stays
/// the locked `[3]` v0.3 snapshot. **Immutable at v0.1**: changing the string
/// OR the schema shape requires a v0.2 URI bump + migration doc + in-toto
/// vetted-catalog re-submission per CLAUDE.md §4. Planning contract:
/// `docs/diagrams/binding/model-binding-and-chain-walk.md`.
pub const MODEL_BINDING_PREDICATE_TYPE: &str =
    "https://attestrum.com/attestation/model-binding/v0.1";

// ============================================================================
// Errors
// ============================================================================

/// Crate-wide error kind.
#[derive(Debug, Error)]
pub enum AttestrumAttestError {
    /// in-toto v1 Statement's `_type` field does not match the spec-mandated
    /// const [`IN_TOTO_STATEMENT_V1_TYPE_URI`].
    #[error("in-toto _type mismatch: expected `{expected}`, got `{actual}`")]
    InTotoTypeMismatch {
        expected: &'static str,
        actual: String,
    },

    /// A proof predicate's `proof_type` const discriminator field has the
    /// wrong value (e.g. an `InclusionProofPredicate` was deserialized from
    /// a payload with `proofType: "non-inclusion"`).
    #[error("proof_type mismatch: expected `{expected}`, got `{actual}`")]
    ProofTypeMismatch {
        expected: &'static str,
        actual: String,
    },

    /// A `NonInclusionProofPredicate` has a `boundary_case` that requires a
    /// neighbor which is `None`. Per the E1.5 cross-check schema lock:
    /// `Interior` requires BOTH neighbors; `BeforeFirst` requires only the
    /// right neighbor; `AfterLast` requires only the left neighbor.
    #[error("boundary_case `{case:?}` is missing a required neighbor")]
    BoundaryCaseNeighborMissing { case: BoundaryCase },

    /// JSON serialization or deserialization failed.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error during sign output write or canonicalize input read.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// `sigstore::bundle::sign::SigningContext::production()` failed —
    /// typically TUF root fetch failure or network error against the
    /// public-good roots.
    #[error("sigstore SigningContext failed: {0}")]
    SigstoreContext(String),

    /// OIDC id_token parsing or validation failed inside sigstore-rs.
    #[error("sigstore IdentityToken failed: {0}")]
    SigstoreIdentityToken(String),

    /// `SigningContext::blocking_signer()` failed — typically Fulcio CSR
    /// rejection (OIDC identity not allowed) or transient network error.
    #[error("sigstore signing session failed: {0}")]
    SigstoreSession(String),

    /// `SigningSession::sign()` failed — typically Rekor v2 submission
    /// rejection or transient network error during transparency-log entry.
    #[error("sigstore sign failed: {0}")]
    SigstoreSign(String),

    /// `dsse_sign::sign_dsse()` failed — typically Rekor v1 `dsse@0.0.1`
    /// submission rejection, transient network error during the
    /// transparency-log entry, ECDSA-P256 signing failure, or DSSE
    /// envelope serialization failure on the fork side. Distinct from
    /// [`Self::SigstoreSign`] (which is the v0.2 + `MessageSignature`
    /// code path) so error logs can distinguish which sign primitive
    /// failed during the X→Y hybrid rollout window.
    #[error("dsse sign failed: {0}")]
    DsseSign(String),

    /// `sigstore::bundle::verify::blocking::Verifier::verify()` failed —
    /// cryptographic verification rejected the bundle (cert chain bad,
    /// signature mismatch, Rekor inclusion proof bad, RFC3161 timestamp
    /// outside cert validity window). E4 surface for sign-side
    /// SigstoreSign's verify-side equivalent.
    #[error("sigstore verify failed: {0}")]
    SigstoreVerify(String),

    /// Failed to extract the identity-pair (SAN + OIDC issuer) from the
    /// bundle's leaf cert. Reasons: no verificationMaterial, malformed
    /// DER, no SAN extension, no recognised SAN entry type, no Fulcio
    /// OIDC-issuer extension (tried both OID 57264.1.8 v1 + 57264.1.1
    /// legacy).
    #[error("identity extraction failed: {0}")]
    IdentityExtractionFailed(String),

    /// Cryptographic verification succeeded, but the bundle's extracted
    /// identity does not satisfy the operator-supplied regex policy.
    /// Distinct from [`Self::SigstoreVerify`] so the verify-side
    /// lifecycle can map this to Exit 6 (verification failure) while
    /// distinguishing "the bundle is malformed" from "the bundle is
    /// well-formed but signed by the wrong identity."
    #[error(
        "identity policy mismatch: extracted (identity={extracted_identity}, issuer={extracted_issuer}) did not match (identity_regex={identity_regex}, issuer_regex={issuer_regex})"
    )]
    IdentityPolicyMismatch {
        extracted_identity: String,
        extracted_issuer: String,
        identity_regex: String,
        issuer_regex: String,
    },

    /// Cryptographic verification succeeded, but the in-toto Statement's
    /// predicate does not deserialize as a [`TrainingCorpusPredicate`].
    /// E4's lightweight Exit 8 path — the Rust types ARE the v0.3 schema
    /// (schemars-derived), so if `serde_json::from_value::<TrainingCorpusPredicate>`
    /// fails, the predicate doesn't satisfy the published schema.
    #[error("predicate validation failed: {0}")]
    PredicateValidationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_uri_constants_match_path_a_brief() {
        assert_eq!(
            TRAINING_CORPUS_PREDICATE_TYPE,
            "https://attestrum.com/attestation/training-corpus/v0.3"
        );
        assert_eq!(
            INCLUSION_PROOF_PREDICATE_TYPE,
            "https://attestrum.com/attestation/inclusion-proof/v0.3"
        );
        assert_eq!(
            NON_INCLUSION_PROOF_PREDICATE_TYPE,
            "https://attestrum.com/attestation/non-inclusion-proof/v0.3"
        );
    }

    #[test]
    fn all_predicate_types_lists_three_in_canonical_order() {
        assert_eq!(ALL_PREDICATE_TYPES.len(), 3);
        assert_eq!(ALL_PREDICATE_TYPES[0], TRAINING_CORPUS_PREDICATE_TYPE);
        assert_eq!(ALL_PREDICATE_TYPES[1], INCLUSION_PROOF_PREDICATE_TYPE);
        assert_eq!(ALL_PREDICATE_TYPES[2], NON_INCLUSION_PROOF_PREDICATE_TYPE);
    }

    #[test]
    fn training_corpus_predicate_can_wrap_in_in_toto_statement() {
        let pred = TrainingCorpusPredicate {
            attestrum_version: "v0.0.1".to_string(),
            builder_version: "attestrum-cli/0.0.1".to_string(),
            built_at: "2026-05-24T18:00:00Z".to_string(),
            determinism: DeterminismFields {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                seed: "1748041200".to_string(),
                manifest_schema_version: "1".to_string(),
            },
            manifest: ManifestRef {
                uri: "attestrum://corpus/manifest.parquet".to_string(),
                digest_set: DigestMap {
                    blake3: "a".repeat(64),
                    sha256: "b".repeat(64),
                },
                row_count: 1000,
                byte_count: 74_564,
            },
            merkle_root: "c".repeat(64),
            merkle_algorithm: "blake3-rfc6962".to_string(),
            ruleset_mode: RulesetMode::Strict,
            ruleset_id: "attestrum-default".to_string(),
            ruleset_version: "v0.1.0".to_string(),
            signal_coverage: SignalCoverage::default(),
            licensing_posture: LicensingPosture::AllOpenLicensed,
            license_inventory: vec![],
            takedown_contact: None,
            dataset_homepage: None,
            publication_intent: None,
            total_compute: None,
            training_cost: None,
            model_name: None,
        };
        let predicate_value = serde_json::to_value(&pred).unwrap();
        let subject = Subject {
            name: "manifest.parquet".to_string(),
            digest: DigestMap {
                blake3: "a".repeat(64),
                sha256: "b".repeat(64),
            },
        };
        let stmt = InTotoStatement::new(
            TRAINING_CORPUS_PREDICATE_TYPE,
            vec![subject],
            predicate_value,
        );
        stmt.validate().unwrap();
        assert_eq!(stmt.predicate_type, TRAINING_CORPUS_PREDICATE_TYPE);
        // canonical_json renders successfully and is reproducible.
        let canonical = stmt.canonical_json().unwrap();
        assert!(canonical.contains(TRAINING_CORPUS_PREDICATE_TYPE));
        assert_eq!(canonical, stmt.canonical_json().unwrap());
    }
}
