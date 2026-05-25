//! Three in-toto v1 predicate types: training-corpus (populated this sprint),
//! inclusion-proof + non-inclusion-proof (frozen shape, `build()` constructors
//! arrive in Sprint 5 with `attestrum prove`).
//!
//! Field shapes match `docs/diagrams/sprint-4/predicate-three-types.md`
//! (revised at E1.5 per `docs/cross-checks/e1.5/resolution.md`). The
//! JSON-Schema files derived from these types via `schemars` (E2.5) are the
//! published authoritative shapes at the three `v0.3.schema.json` URLs.
//!
//! **E3.6 v0.1 → v0.3 bump** (determinism hardening): 10 `f32` fields became
//! `u32` parts-per-million (PPM) to make their JSON wire form byte-
//! deterministic across the 4-target CI matrix. See
//! `docs/migration/v0.2-to-v0.3-attestrum-rebrand.md`.

use serde::{Deserialize, Serialize};

use crate::AttestrumAttestError;

// ============================================================================
// Shared building blocks
// ============================================================================

/// Algorithm-qualified digest carrying BOTH BLAKE3 (Attestrum-native) and SHA-256
/// (Sigstore/in-toto interop) per BUILD-PLAN §3.4. Both required. Hex is
/// lowercase 64-char.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DigestMap {
    pub blake3: String,
    pub sha256: String,
}

/// in-toto v1 Subject — `name` plus algorithm-qualified digest map. Used by
/// the wrapping Statement's `subject[]` and embedded in
/// `InclusionProofPredicate::matched_subject`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Subject {
    pub name: String,
    pub digest: DigestMap,
}

// ============================================================================
// training-corpus/v0.3
// ============================================================================

/// `https://attestrum.com/attestation/training-corpus/v0.3` predicate payload.
/// Built by `attestrum sign` from a sealed manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrainingCorpusPredicate {
    pub attestrum_version: String,
    pub builder_version: String,
    pub built_at: String,
    pub determinism: DeterminismFields,
    pub manifest: ManifestRef,
    pub merkle_root: String,
    pub merkle_algorithm: String,
    pub ruleset_mode: RulesetMode,
    pub ruleset_id: String,
    pub ruleset_version: String,
    pub signal_coverage: SignalCoverage,
    pub licensing_posture: LicensingPosture,
    pub license_inventory: Vec<LicenseInventoryEntry>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub takedown_contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_intent: Option<PublicationIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_compute: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_cost: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeterminismFields {
    pub target_triple: String,
    pub seed: String,
    pub manifest_schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRef {
    pub uri: String,
    pub digest_set: DigestMap,
    pub row_count: u64,
    pub byte_count: u64,
}

/// All nine signal-coverage ratios as parts-per-million (PPM), each in
/// `0..=1_000_000` representing `0.0..=1.0` coverage. `Option<u32>` here so
/// a signal that was never evaluated stays absent rather than appearing as
/// `0` (which means "evaluated and zero coverage").
///
/// **E3.6 v0.3 schema bump**: changed from `Option<f32>` to `Option<u32>` PPM
/// for cross-target byte determinism — float JSON serialization is platform-
/// nondeterministic (rounding, denormal handling, NaN bits). See
/// `docs/migration/v0.2-to-v0.3-attestrum-rebrand.md`. Verifier-side: read the
/// integer value, divide by `1_000_000.0`, format for human display.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignalCoverage {
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub robots_txt: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_txt: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tdm_rep: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aipref: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iptc_plus: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c2pa: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsl: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liccium: Option<u32>,
    /// Parts per million; 0..=1_000_000 maps 0.0..=1.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloudflare: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInventoryEntry {
    pub spdx_id: String,
    pub byte_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Per PATH-A-BRIEF §3.1: `"strict" | "audit-only" | "permissive"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RulesetMode {
    Strict,
    AuditOnly,
    Permissive,
}

/// Per PATH-A-BRIEF §3.1: `"allOpenLicensed" | "mixedLicensed" | "allLicensed" | "undisclosed"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LicensingPosture {
    AllOpenLicensed,
    MixedLicensed,
    AllLicensed,
    Undisclosed,
}

/// Per PATH-A-BRIEF §3.1: `"huggingface-hub" | "zenodo" | "github-release" | "eu-ai-office" | "private"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PublicationIntent {
    HuggingFaceHub,
    Zenodo,
    GitHubRelease,
    EuAiOffice,
    Private,
}

// ============================================================================
// inclusion-proof/v0.3
// ============================================================================

/// `https://attestrum.com/attestation/inclusion-proof/v0.3` predicate payload.
/// Frozen shape locked at E1.5 cross-check; `build()` constructor lands in
/// Sprint 5 alongside `attestrum prove`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InclusionProofPredicate {
    /// Discriminator. Must be `"inclusion"`. Validated via [`Self::validate`].
    pub proof_type: String,
    pub corpus: CorpusRef,
    pub query_fingerprint: serde_json::Value,
    pub match_evidence: MatchEvidence,
    pub tree_size: u64,
    pub leaf_count: u64,
    pub leaf_hash: String,
    pub hash_algorithm: String,
    pub audit_path: Vec<String>,
    pub leaf_index: u64,
    pub matched_subject: Subject,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_generator_identity: Option<String>,
}

impl InclusionProofPredicate {
    /// Const value of the `proof_type` discriminator field.
    pub const PROOF_TYPE_VALUE: &'static str = "inclusion";

    /// Validates the const-discriminator field per the schema.
    pub fn validate(&self) -> Result<(), AttestrumAttestError> {
        if self.proof_type != Self::PROOF_TYPE_VALUE {
            return Err(AttestrumAttestError::ProofTypeMismatch {
                expected: Self::PROOF_TYPE_VALUE,
                actual: self.proof_type.clone(),
            });
        }
        Ok(())
    }
}

/// Reference to the corpus a proof attests against. The `attestation_digest`
/// is the digest of the corpus's signed in-toto Statement (the bundle's DSSE
/// payload), algorithm-qualified per the E1.5 cross-check finding that raw
/// hex without algorithm is ambiguous for verifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorpusRef {
    pub manifest_uri: String,
    pub merkle_root: String,
    pub attestation_digest: DigestMap,
}

/// Per-mode evidence for an inclusion match. Exact-hash variants carry no
/// evidence struct (the match itself is the proof); fuzzy variants carry
/// mode-specific parameters.
///
/// Serialized with the variant discriminator at field `matchMode`:
///
/// ```json
/// { "matchMode": "exact-blake3" }
/// { "matchMode": "iscc", "compositeDistance": 3 }
/// { "matchMode": "perceptual", "hammingDistance": 4, "threshold": 6 }
/// { "matchMode": "minhash", "jaccard": 920000, "ngramSize": 5 }
/// ```
///
/// **E3.6 v0.3 schema bump**: `jaccard` is `u32` parts-per-million (PPM) in
/// `0..=1_000_000` mapping `0.0..=1.0` similarity — was `f32` at v0.1. See
/// `docs/migration/v0.2-to-v0.3-attestrum-rebrand.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "matchMode", rename_all = "kebab-case")]
pub enum MatchEvidence {
    ExactBlake3,
    ExactSha256,
    Iscc(IsccEvidence),
    Perceptual(PerceptualEvidence),
    MinHash(MinHashEvidence),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsccEvidence {
    pub composite_distance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerceptualEvidence {
    pub hamming_distance: u32,
    pub threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MinHashEvidence {
    /// Jaccard similarity as parts per million; 0..=1_000_000 maps 0.0..=1.0.
    /// E3.6 v0.3 bump: was `f32` at v0.1, changed for cross-target byte
    /// determinism (float JSON serialization is platform-nondeterministic).
    pub jaccard: u32,
    pub ngram_size: u32,
}

// ============================================================================
// non-inclusion-proof/v0.3
// ============================================================================

/// `https://attestrum.com/attestation/non-inclusion-proof/v0.3` predicate
/// payload. Sorted-Merkle adjacent-leaves technique with explicit boundary-
/// case handling per the E1.5 cross-check finding that requiring BOTH
/// neighbors blocks first-leaf and last-leaf proofs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NonInclusionProofPredicate {
    /// Discriminator. Must be `"non-inclusion"`. Validated via [`Self::validate`].
    pub proof_type: String,
    pub corpus: CorpusRef,
    pub query_fingerprint: serde_json::Value,
    pub tree_size: u64,
    pub hash_algorithm: String,
    pub query_key: String,
    pub boundary_case: BoundaryCase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_neighbor: Option<Neighbor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_neighbor: Option<Neighbor>,

    pub sorted_assertion: SortedAssertion,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_generator_identity: Option<String>,
}

impl NonInclusionProofPredicate {
    /// Const value of the `proof_type` discriminator field.
    pub const PROOF_TYPE_VALUE: &'static str = "non-inclusion";

    /// Validates the const-discriminator field + the boundary-case neighbor
    /// requirement per the schema:
    ///
    /// - `Interior`: both neighbors required.
    /// - `BeforeFirst`: only `right_neighbor` required.
    /// - `AfterLast`: only `left_neighbor` required.
    pub fn validate(&self) -> Result<(), AttestrumAttestError> {
        if self.proof_type != Self::PROOF_TYPE_VALUE {
            return Err(AttestrumAttestError::ProofTypeMismatch {
                expected: Self::PROOF_TYPE_VALUE,
                actual: self.proof_type.clone(),
            });
        }
        match self.boundary_case {
            BoundaryCase::Interior => {
                if self.left_neighbor.is_none() || self.right_neighbor.is_none() {
                    return Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                        case: self.boundary_case,
                    });
                }
            }
            BoundaryCase::BeforeFirst => {
                if self.right_neighbor.is_none() {
                    return Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                        case: self.boundary_case,
                    });
                }
            }
            BoundaryCase::AfterLast => {
                if self.left_neighbor.is_none() {
                    return Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                        case: self.boundary_case,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Where the query falls in the sort order. Resolves the universal-both-
/// neighbors-required brittleness flagged by the E1.5 cross-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryCase {
    Interior,
    BeforeFirst,
    AfterLast,
}

/// A leaf adjacent to the query in the sorted order. Carries its own inclusion
/// proof (`inclusion_proof_audit_path`) so the verifier can independently
/// confirm the neighbor is in the tree before checking adjacency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Neighbor {
    pub leaf_hash: String,
    pub ordering_key: String,
    pub leaf_index: u64,
    pub inclusion_proof_audit_path: Vec<String>,
}

/// Structured sort-order assertion. Replaces the E1-era const-string
/// comparator that the verifier could not structurally check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SortedAssertion {
    /// Const `"blake3-bytewise-ascending"` at v0.1.
    pub ordering: String,
    /// Const `"leftIndex + 1 == rightIndex"` at v0.1.
    pub adjacency_invariant: String,
    /// Free-text describing how duplicate leaves (same hash, different
    /// `input_ordinal` in the manifest) are handled by this proof. Required
    /// per E1.5 cross-check finding that multiset behavior was undefined.
    pub duplicate_leaf_policy: String,
}

impl SortedAssertion {
    /// Const value of the `ordering` field for v0.1.
    pub const ORDERING_V0_1: &'static str = "blake3-bytewise-ascending";
    /// Const value of the `adjacency_invariant` field for v0.1.
    pub const ADJACENCY_INVARIANT_V0_1: &'static str = "leftIndex + 1 == rightIndex";
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_digest_map() -> DigestMap {
        DigestMap {
            blake3: "47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73".to_string(),
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        }
    }

    fn sample_corpus_ref() -> CorpusRef {
        CorpusRef {
            manifest_uri: "attestrum://corpus/manifest.parquet".to_string(),
            merkle_root: "47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73"
                .to_string(),
            attestation_digest: sample_digest_map(),
        }
    }

    #[test]
    fn training_corpus_round_trips_via_serde_json() {
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
                digest_set: sample_digest_map(),
                row_count: 1000,
                byte_count: 74_564,
            },
            merkle_root: "47db4aaf7de8c179bdb9662181c76b8b874ce15a49158aad6d8b761e80f96d73"
                .to_string(),
            merkle_algorithm: "blake3-rfc6962".to_string(),
            ruleset_mode: RulesetMode::Strict,
            ruleset_id: "attestrum-default".to_string(),
            ruleset_version: "v0.1.0".to_string(),
            signal_coverage: SignalCoverage {
                // v0.3: PPM, was Some(0.95) at v0.1.
                robots_txt: Some(950_000),
                ..Default::default()
            },
            licensing_posture: LicensingPosture::AllOpenLicensed,
            license_inventory: vec![LicenseInventoryEntry {
                spdx_id: "Apache-2.0".to_string(),
                byte_count: 50_000,
                row_count: Some(500),
                notes: None,
            }],
            takedown_contact: Some("mailto:takedown@example.org".to_string()),
            dataset_homepage: None,
            publication_intent: Some(PublicationIntent::HuggingFaceHub),
            total_compute: None,
            training_cost: None,
            model_name: None,
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: TrainingCorpusPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(pred, back);
    }

    #[test]
    fn training_corpus_emits_camel_case_keys_and_kebab_case_enum_values() {
        let pred = TrainingCorpusPredicate {
            attestrum_version: "v0.0.1".to_string(),
            builder_version: "x".to_string(),
            built_at: "2026-05-24T18:00:00Z".to_string(),
            determinism: DeterminismFields {
                target_triple: "x".to_string(),
                seed: "x".to_string(),
                manifest_schema_version: "1".to_string(),
            },
            manifest: ManifestRef {
                uri: "x".to_string(),
                digest_set: sample_digest_map(),
                row_count: 0,
                byte_count: 0,
            },
            merkle_root: "0".repeat(64),
            merkle_algorithm: "blake3-rfc6962".to_string(),
            ruleset_mode: RulesetMode::AuditOnly,
            ruleset_id: "x".to_string(),
            ruleset_version: "x".to_string(),
            signal_coverage: SignalCoverage::default(),
            licensing_posture: LicensingPosture::MixedLicensed,
            license_inventory: vec![],
            takedown_contact: None,
            dataset_homepage: None,
            publication_intent: Some(PublicationIntent::EuAiOffice),
            total_compute: None,
            training_cost: None,
            model_name: None,
        };
        let json = serde_json::to_string(&pred).unwrap();
        // camelCase top-level keys
        assert!(json.contains("\"attestrumVersion\""));
        assert!(json.contains("\"builderVersion\""));
        assert!(json.contains("\"rulesetMode\""));
        assert!(json.contains("\"licensingPosture\""));
        assert!(json.contains("\"merkleAlgorithm\""));
        // kebab-case enum value for RulesetMode (per PATH-A-BRIEF §3.1)
        assert!(json.contains("\"audit-only\""));
        // camelCase enum value for LicensingPosture (per PATH-A-BRIEF §3.1)
        assert!(json.contains("\"mixedLicensed\""));
        // kebab-case enum value for PublicationIntent (per PATH-A-BRIEF §3.1)
        assert!(json.contains("\"eu-ai-office\""));
        // Optional None fields are omitted
        assert!(!json.contains("\"takedownContact\""));
        assert!(!json.contains("\"totalCompute\""));
    }

    #[test]
    fn inclusion_proof_validates_proof_type_discriminator() {
        let pred = InclusionProofPredicate {
            proof_type: "inclusion".to_string(),
            corpus: sample_corpus_ref(),
            query_fingerprint: json!({"blake3": "deadbeef"}),
            match_evidence: MatchEvidence::ExactBlake3,
            tree_size: 1000,
            leaf_count: 1000,
            leaf_hash: "0".repeat(64),
            hash_algorithm: "blake3-rfc6962".to_string(),
            audit_path: vec!["0".repeat(64); 10],
            leaf_index: 42,
            matched_subject: Subject {
                name: "doc-042".to_string(),
                digest: sample_digest_map(),
            },
            proof_generated_at: None,
            proof_generator_identity: None,
        };
        pred.validate().unwrap();

        let bad = InclusionProofPredicate {
            proof_type: "non-inclusion".to_string(),
            ..pred
        };
        match bad.validate() {
            Err(AttestrumAttestError::ProofTypeMismatch { expected, actual }) => {
                assert_eq!(expected, "inclusion");
                assert_eq!(actual, "non-inclusion");
            }
            other => panic!("expected ProofTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn non_inclusion_proof_validates_boundary_case_neighbor_requirements() {
        let neighbor = Neighbor {
            leaf_hash: "0".repeat(64),
            ordering_key: "0".repeat(64),
            leaf_index: 5,
            inclusion_proof_audit_path: vec!["0".repeat(64); 10],
        };
        let sorted = SortedAssertion {
            ordering: SortedAssertion::ORDERING_V0_1.to_string(),
            adjacency_invariant: SortedAssertion::ADJACENCY_INVARIANT_V0_1.to_string(),
            duplicate_leaf_policy: "first-leaf wins; subsequent duplicates skipped".to_string(),
        };

        // Interior: both neighbors required.
        let interior_ok = NonInclusionProofPredicate {
            proof_type: "non-inclusion".to_string(),
            corpus: sample_corpus_ref(),
            query_fingerprint: json!({"blake3": "deadbeef"}),
            tree_size: 1000,
            hash_algorithm: "blake3-rfc6962".to_string(),
            query_key: "1".repeat(64),
            boundary_case: BoundaryCase::Interior,
            left_neighbor: Some(neighbor.clone()),
            right_neighbor: Some(neighbor.clone()),
            sorted_assertion: sorted.clone(),
            proof_generated_at: None,
            proof_generator_identity: None,
        };
        interior_ok.validate().unwrap();

        let interior_bad = NonInclusionProofPredicate {
            right_neighbor: None,
            ..interior_ok.clone()
        };
        assert!(matches!(
            interior_bad.validate(),
            Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                case: BoundaryCase::Interior
            })
        ));

        // BeforeFirst: only right_neighbor required.
        let before_first_ok = NonInclusionProofPredicate {
            boundary_case: BoundaryCase::BeforeFirst,
            left_neighbor: None,
            right_neighbor: Some(neighbor.clone()),
            ..interior_ok.clone()
        };
        before_first_ok.validate().unwrap();

        let before_first_bad = NonInclusionProofPredicate {
            boundary_case: BoundaryCase::BeforeFirst,
            left_neighbor: None,
            right_neighbor: None,
            ..interior_ok.clone()
        };
        assert!(matches!(
            before_first_bad.validate(),
            Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                case: BoundaryCase::BeforeFirst
            })
        ));

        // AfterLast: only left_neighbor required.
        let after_last_ok = NonInclusionProofPredicate {
            boundary_case: BoundaryCase::AfterLast,
            left_neighbor: Some(neighbor.clone()),
            right_neighbor: None,
            ..interior_ok.clone()
        };
        after_last_ok.validate().unwrap();

        let after_last_bad = NonInclusionProofPredicate {
            boundary_case: BoundaryCase::AfterLast,
            left_neighbor: None,
            right_neighbor: None,
            ..interior_ok
        };
        assert!(matches!(
            after_last_bad.validate(),
            Err(AttestrumAttestError::BoundaryCaseNeighborMissing {
                case: BoundaryCase::AfterLast
            })
        ));
    }

    #[test]
    fn non_inclusion_proof_rejects_wrong_discriminator() {
        let bad = NonInclusionProofPredicate {
            proof_type: "inclusion".to_string(),
            corpus: sample_corpus_ref(),
            query_fingerprint: json!({}),
            tree_size: 0,
            hash_algorithm: "blake3-rfc6962".to_string(),
            query_key: "0".repeat(64),
            boundary_case: BoundaryCase::Interior,
            left_neighbor: None,
            right_neighbor: None,
            sorted_assertion: SortedAssertion {
                ordering: SortedAssertion::ORDERING_V0_1.to_string(),
                adjacency_invariant: SortedAssertion::ADJACENCY_INVARIANT_V0_1.to_string(),
                duplicate_leaf_policy: "x".to_string(),
            },
            proof_generated_at: None,
            proof_generator_identity: None,
        };
        match bad.validate() {
            Err(AttestrumAttestError::ProofTypeMismatch { expected, actual }) => {
                assert_eq!(expected, "non-inclusion");
                assert_eq!(actual, "inclusion");
            }
            other => panic!("expected ProofTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn match_evidence_serializes_with_tagged_discriminator() {
        let exact = MatchEvidence::ExactBlake3;
        assert_eq!(
            serde_json::to_string(&exact).unwrap(),
            r#"{"matchMode":"exact-blake3"}"#
        );
        let perceptual = MatchEvidence::Perceptual(PerceptualEvidence {
            hamming_distance: 4,
            threshold: 6,
        });
        let json = serde_json::to_string(&perceptual).unwrap();
        assert!(json.contains("\"matchMode\":\"perceptual\""));
        assert!(json.contains("\"hammingDistance\":4"));
        assert!(json.contains("\"threshold\":6"));
    }

    #[test]
    fn ruleset_mode_kebab_case_round_trip() {
        for (variant, expected) in [
            (RulesetMode::Strict, "\"strict\""),
            (RulesetMode::AuditOnly, "\"audit-only\""),
            (RulesetMode::Permissive, "\"permissive\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
            let back: RulesetMode = serde_json::from_str(expected).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn boundary_case_kebab_case_round_trip() {
        for (variant, expected) in [
            (BoundaryCase::Interior, "\"interior\""),
            (BoundaryCase::BeforeFirst, "\"before-first\""),
            (BoundaryCase::AfterLast, "\"after-last\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
            let back: BoundaryCase = serde_json::from_str(expected).unwrap();
            assert_eq!(back, variant);
        }
    }
}
