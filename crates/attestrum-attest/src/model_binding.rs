//! `model-binding/v0.1` predicate type (corpus-to-model binding).
//!
//! A signed training-corpus attestation proves "corpus C existed with Merkle
//! root R" — not "C is what trained model M." This predicate closes that gap:
//! an in-toto v1 Statement, SLSA-shaped, with the **model as `subject`** (the
//! product) and the **corpus attestation(s) as materials** (the inputs), under
//! the predicate URI [`crate::MODEL_BINDING_PREDICATE_TYPE`].
//!
//! **PROTECTED** (CLAUDE.md §4): the URI is `model-binding/v0.1`, a separate
//! generation from the frozen v0.3 corpus/proof predicate family — so
//! [`crate::ALL_PREDICATE_TYPES`] stays `[3]` and this URI is a standalone
//! const (D2-A). Any change to the URI string or the JSON-Schema shape of this
//! payload requires a `v0.2` URI bump, a migration document, and an in-toto
//! vetted-catalog re-submission. Planning contract:
//! `docs/diagrams/binding/model-binding-and-chain-walk.md`.
//!
//! **Honest ceiling:** this is an *attestation, not a proof-of-training*. The
//! cryptography guarantees integrity + timestamp + identity + verifiable
//! membership against C — NOT the truth of the training claim. Stated plainly
//! because it is foundational to how the artifact may be represented to a
//! design partner.

use serde::{Deserialize, Serialize};

use crate::predicate::DigestMap;

/// `https://attestrum.com/attestation/model-binding/v0.1` predicate payload.
///
/// `subject` (on the wrapping [`crate::InTotoStatement`]) is the model — its
/// weights-manifest digest, with the model-card URI as the subject `name`.
/// `corpora` are the materials: the training-corpus attestation(s) the trainer
/// claims produced the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelBindingPredicate {
    /// The training-corpus attestation(s) bound to the model. A SET so a model
    /// trained on multiple corpora (e.g. pretraining + finetuning) binds all of
    /// them; the chain walk then answers "X in at least one corpus that trained
    /// M."
    pub corpora: Vec<CorpusBindingRef>,
    /// The model being bound (identity + digest + optional OpenSSF signing
    /// bundle reference).
    pub model: ModelRef,
    /// Contemporaneous training metadata captured at bind time.
    pub training: TrainingMeta,
    /// Informational builder string per CLAUDE.md §12 (vendor neutrality — the
    /// only place the "attestrum" string appears in emitted structure besides
    /// the predicate URI prefix).
    pub builder_version: String,
}

/// A reference, by digest, to one training-corpus attestation that the model is
/// bound to. The corpus attestation itself is unchanged — this is a material
/// pointer, signed *after* training (the corpus was sealed *before* training,
/// so it cannot itself carry the model digest).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorpusBindingRef {
    /// BLAKE3 + SHA-256 digest of the corpus's in-toto Statement bytes (the
    /// canonical-JSON payload), as computed by
    /// [`crate::attestation_digest_of_bundle`]. Lets a verifier confirm the
    /// supplied corpus attestation is *the* one the model was bound to.
    pub attestation_digest: DigestMap,
    /// The corpus's RFC 6962 BLAKE3 Merkle root (hex), lifted from the
    /// training-corpus predicate. Redundant with the corpus attestation but
    /// carried here so the binding is self-describing at a glance.
    pub merkle_root: String,
    /// The corpus manifest's digest (from the training-corpus predicate's
    /// `manifest.digestSet`).
    pub manifest_digest: DigestMap,
    /// Free-text role of this corpus in producing the model. A `String` at v0.1
    /// (D4-A) rather than a closed enum, so an evolving training-method taxonomy
    /// is not trapped in a PROTECTED wire format. Recommended vocabulary:
    /// `"pretraining"`, `"finetuning"`, `"rlhf"`, `"distillation"`,
    /// `"continued-pretraining"`.
    pub role: String,
}

/// Identifies the model being bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Soft identifier — typically the model-card / release URI.
    pub identity: String,
    /// The model's weights-manifest digest (OpenSSF Model Signing style — a
    /// digest of the signed manifest of the model's files). This is also the
    /// digest carried in the wrapping Statement's `subject[].digest`.
    pub weights_manifest_digest: DigestMap,
    /// Optional reference to the model's own OpenSSF/Sigstore signing bundle
    /// (by URI or digest). **Recorded, not verified at v0.1** (D3-A): Attestrum
    /// does not verify the model's own signature here; verify-if-present is
    /// deferred to v0.2. When present, the binding composes two supply chains —
    /// the corpus supply chain and the model supply chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_bundle_ref: Option<String>,
}

/// Contemporaneous training metadata. Captured at bind time, which is *after*
/// training (unlike the corpus attestation, sealed before training).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrainingMeta {
    /// Optional digest of the training configuration (hyperparameters, recipe).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_digest: Option<DigestMap>,
    /// Who made the binding claim (e.g. an org name or Sigstore identity). The
    /// authoritative identity binding is the signing bundle's leaf cert; this
    /// is the human-readable claimant.
    pub builder_identity: String,
    /// RFC 3339 timestamp the binding was made (deterministic — derived from
    /// `source_date_epoch`).
    pub bound_at: String,
    /// Source-date-epoch (Unix seconds) used to derive `bound_at`, mirroring the
    /// determinism convention used across Attestrum.
    pub source_date_epoch: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statement::InTotoStatement;
    use crate::{predicate::Subject, MODEL_BINDING_PREDICATE_TYPE};

    fn digest_map(b: u8) -> DigestMap {
        DigestMap {
            blake3: format!("{:02x}", b).repeat(32),
            sha256: format!("{:02x}", b ^ 0xff).repeat(32),
        }
    }

    fn sample_predicate() -> ModelBindingPredicate {
        ModelBindingPredicate {
            corpora: vec![CorpusBindingRef {
                attestation_digest: digest_map(1),
                merkle_root: "a".repeat(64),
                manifest_digest: digest_map(2),
                role: "pretraining".to_string(),
            }],
            model: ModelRef {
                identity: "https://huggingface.co/acme/model-m".to_string(),
                weights_manifest_digest: digest_map(3),
                signing_bundle_ref: Some(
                    "https://huggingface.co/acme/model-m/model.sig".to_string(),
                ),
            },
            training: TrainingMeta {
                config_digest: Some(digest_map(4)),
                builder_identity: "Acme AI".to_string(),
                bound_at: "2026-05-29T00:00:00Z".to_string(),
                source_date_epoch: 1_748_476_800,
            },
            builder_version: "attestrum-cli/0.0.1".to_string(),
        }
    }

    #[test]
    fn model_binding_round_trips_via_serde_json() {
        let pred = sample_predicate();
        let json = serde_json::to_string(&pred).unwrap();
        let back: ModelBindingPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(pred, back);
    }

    #[test]
    fn model_binding_emits_camel_case_keys() {
        let pred = sample_predicate();
        let json = serde_json::to_string(&pred).unwrap();
        assert!(json.contains("\"attestationDigest\""));
        assert!(json.contains("\"merkleRoot\""));
        assert!(json.contains("\"manifestDigest\""));
        assert!(json.contains("\"weightsManifestDigest\""));
        assert!(json.contains("\"signingBundleRef\""));
        assert!(json.contains("\"configDigest\""));
        assert!(json.contains("\"builderIdentity\""));
        assert!(json.contains("\"boundAt\""));
        assert!(json.contains("\"sourceDateEpoch\""));
        assert!(json.contains("\"builderVersion\""));
    }

    #[test]
    fn optional_fields_omitted_when_none() {
        let mut pred = sample_predicate();
        pred.model.signing_bundle_ref = None;
        pred.training.config_digest = None;
        let json = serde_json::to_string(&pred).unwrap();
        assert!(!json.contains("\"signingBundleRef\""));
        assert!(!json.contains("\"configDigest\""));
    }

    #[test]
    fn model_binding_wraps_in_in_toto_statement_with_model_subject() {
        let pred = sample_predicate();
        let predicate_value = serde_json::to_value(&pred).unwrap();
        let subject = Subject {
            name: pred.model.identity.clone(),
            digest: pred.model.weights_manifest_digest.clone(),
        };
        let stmt =
            InTotoStatement::new(MODEL_BINDING_PREDICATE_TYPE, vec![subject], predicate_value);
        stmt.validate().unwrap();
        assert_eq!(stmt.predicate_type, MODEL_BINDING_PREDICATE_TYPE);
        // canonical_json renders and is reproducible.
        let canonical = stmt.canonical_json().unwrap();
        assert_eq!(canonical, stmt.canonical_json().unwrap());
        assert!(canonical.contains(MODEL_BINDING_PREDICATE_TYPE));
    }
}
