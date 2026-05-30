//! `attestrum-bind` — corpus-to-model binding.
//!
//! A signed Attestrum corpus attestation proves "corpus C existed with Merkle
//! root R" — **not** "C is what trained model M." This crate closes that gap.
//!
//! - [`bind`] emits a `model-binding/v0.1` in-toto Statement (optionally
//!   Sigstore-signed) with the **model as the subject** and one or more
//!   **training-corpus attestations as materials**.
//! - `walk_chain` (the keystone, lands at Commit 5) verifies the full chain:
//!   model digest → signed binding → signed corpus attestation → live
//!   membership proof of a work X.
//!
//! **Honest ceiling:** binding is an *attestation, not a proof-of-training*. The
//! cryptography guarantees integrity + timestamp + identity + verifiable
//! membership against the corpus — NOT the truth of the training claim itself.
//! State this plainly to any design partner.

use std::path::PathBuf;

use attestrum_attest::{
    attestation_digest_of_statement, statement_from_bundle, AttestrumAttestError, CorpusBindingRef,
    DigestMap, InTotoStatement, ModelBindingPredicate, ModelRef, SignRequest, Subject,
    TrainingCorpusPredicate, TrainingMeta, MODEL_BINDING_PREDICATE_TYPE,
};

/// One training-corpus attestation to bind to the model, identified by its
/// bundle path (a signed Sigstore Bundle v0.3 or a raw Statement JSON), with
/// the role it played in producing the model.
#[derive(Debug, Clone)]
pub struct BoundCorpus {
    /// Path to the corpus's `training-corpus/v0.3` attestation. Its canonical
    /// Statement bytes are digested into the [`CorpusBindingRef`].
    pub bundle_path: PathBuf,
    /// Free-text role — e.g. `"pretraining"`, `"finetuning"`, `"rlhf"`.
    pub role: String,
}

/// Inputs to [`bind`].
#[derive(Debug, Clone)]
pub struct BindOpts {
    /// The model being bound. Its `weights_manifest_digest` becomes the
    /// Statement subject's digest.
    pub model: ModelRef,
    /// Model-card / release URI — the Statement subject's `name`.
    pub model_card_uri: String,
    /// The corpus attestation(s) the trainer claims produced the model.
    pub corpora: Vec<BoundCorpus>,
    /// Human-readable claimant identity. When signing, the authoritative
    /// identity is the Sigstore leaf cert; this is the displayed claimant.
    pub builder_identity: String,
    /// Optional training-config digest.
    pub config_digest: Option<DigestMap>,
    /// Deterministic timestamp seed; `bound_at` is derived from it.
    pub source_date_epoch: i64,
    /// Informational builder string (CLAUDE.md §12).
    pub builder_version: String,
    /// Sigstore-sign the binding (mirrors `prove`'s optional-sign block). When
    /// true, `oidc_id_token` must be `Some`.
    pub sign: bool,
    /// Raw OIDC JWT for Sigstore Fulcio; required iff `sign` is true.
    pub oidc_id_token: Option<String>,
    /// Workspace dir for the signed-bundle output (defaults to `./.attestrum`).
    pub workspace: Option<PathBuf>,
}

/// Output of [`bind`].
#[derive(Debug, Clone)]
pub struct BindArtifact {
    /// The canonical `model-binding/v0.1` in-toto Statement (always present).
    pub statement: InTotoStatement,
    /// Path to the written Sigstore Bundle v0.3 — `Some` iff `opts.sign`.
    pub bundle_path: Option<PathBuf>,
}

/// Build a `model-binding/v0.1` in-toto Statement linking the model to its
/// training corpora, optionally Sigstore-signed.
///
/// For each corpus: reads its attestation via
/// [`attestrum_attest::statement_from_bundle`], digests the canonical Statement
/// bytes via [`attestrum_attest::attestation_digest_of_statement`] (the same
/// primitive `prove` and `walk_chain` use, so the chain links byte-for-byte),
/// and lifts the `merkleRoot` + `manifest.digestSet` from the training-corpus
/// predicate into a [`CorpusBindingRef`]. The model is the Statement subject;
/// the corpora are the predicate materials.
pub fn bind(opts: &BindOpts) -> Result<BindArtifact, BindError> {
    let mut corpora_refs = Vec::with_capacity(opts.corpora.len());
    for bc in &opts.corpora {
        let stmt = statement_from_bundle(&bc.bundle_path).map_err(BindError::Corpus)?;
        let attestation_digest =
            attestation_digest_of_statement(&stmt).map_err(BindError::Corpus)?;
        let corpus_pred: TrainingCorpusPredicate =
            serde_json::from_value(stmt.predicate.clone()).map_err(BindError::CorpusPredicate)?;
        corpora_refs.push(CorpusBindingRef {
            attestation_digest,
            merkle_root: corpus_pred.merkle_root,
            manifest_digest: corpus_pred.manifest.digest_set,
            role: bc.role.clone(),
        });
    }

    let bound_at = jiff::Timestamp::from_second(opts.source_date_epoch)
        .map_err(|e| BindError::Timestamp(e.to_string()))?
        .to_string();

    let predicate = ModelBindingPredicate {
        corpora: corpora_refs,
        model: opts.model.clone(),
        training: TrainingMeta {
            config_digest: opts.config_digest.clone(),
            builder_identity: opts.builder_identity.clone(),
            bound_at,
            source_date_epoch: opts.source_date_epoch,
        },
        builder_version: opts.builder_version.clone(),
    };

    let subject = Subject {
        name: opts.model_card_uri.clone(),
        digest: opts.model.weights_manifest_digest.clone(),
    };
    let predicate_value = serde_json::to_value(&predicate).map_err(BindError::Serialize)?;
    let statement =
        InTotoStatement::new(MODEL_BINDING_PREDICATE_TYPE, vec![subject], predicate_value);

    let bundle_path = if opts.sign {
        // Mirror prove()'s optional-sign block: require the OIDC token, sign the
        // canonical Statement, write the Sigstore Bundle under <workspace>/bind/.
        let oidc_token = opts.oidc_id_token.clone().ok_or_else(|| {
            BindError::Sign(AttestrumAttestError::SigstoreIdentityToken(
                "BindOpts.oidc_id_token must be Some when BindOpts.sign is true".into(),
            ))
        })?;
        let canonical = statement
            .canonical_json()
            .map_err(BindError::Canonicalize)?;
        let workspace_dir = opts.workspace.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.join(".attestrum"))
                .unwrap_or_else(|_| PathBuf::from(".attestrum"))
        });
        let bundle_dir = workspace_dir.join("bind");
        std::fs::create_dir_all(&bundle_dir).map_err(BindError::Io)?;
        let bundle_out = bundle_dir.join("model-binding.sigstore.json");
        let signed = attestrum_attest::sign(SignRequest {
            statement_payload: canonical.as_bytes(),
            bundle_output_path: &bundle_out,
            oidc_id_token: oidc_token,
        })
        .map_err(BindError::Sign)?;
        Some(signed.bundle_path)
    } else {
        None
    };

    Ok(BindArtifact {
        statement,
        bundle_path,
    })
}

/// Errors from [`bind`].
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// Reading or digesting a corpus attestation failed.
    #[error("corpus attestation read/digest failed: {0}")]
    Corpus(AttestrumAttestError),
    /// A corpus attestation's predicate is not a valid training-corpus payload.
    #[error("corpus statement is not a valid training-corpus predicate: {0}")]
    CorpusPredicate(serde_json::Error),
    /// Deriving `bound_at` from `source_date_epoch` failed.
    #[error("deriving bound_at from source_date_epoch failed: {0}")]
    Timestamp(String),
    /// Serializing the model-binding predicate failed.
    #[error("serializing model-binding predicate failed: {0}")]
    Serialize(serde_json::Error),
    /// Canonicalizing the binding Statement before signing failed.
    #[error("binding statement canonicalization failed: {0}")]
    Canonicalize(AttestrumAttestError),
    /// Writing the signed-bundle workspace dir failed.
    #[error("io: {0}")]
    Io(std::io::Error),
    /// Sigstore signing failed (including a missing OIDC token when signing).
    #[error("sign failed: {0}")]
    Sign(AttestrumAttestError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use attestrum_attest::{
        DeterminismFields, LicensingPosture, ManifestRef, RulesetMode, SignalCoverage,
        TRAINING_CORPUS_PREDICATE_TYPE,
    };

    fn digest_map(b: u8) -> DigestMap {
        DigestMap {
            blake3: format!("{:02x}", b).repeat(32),
            sha256: format!("{:02x}", b ^ 0xff).repeat(32),
        }
    }

    /// Build a minimal valid training-corpus Statement and write its canonical
    /// bytes to a temp file, returning the path.
    fn write_corpus_bundle(name: &str, merkle: u8) -> PathBuf {
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
                digest_set: digest_map(2),
                row_count: 1000,
                byte_count: 74_564,
            },
            merkle_root: format!("{:02x}", merkle).repeat(32),
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
        let subject = Subject {
            name: "manifest.parquet".to_string(),
            digest: digest_map(2),
        };
        let stmt = InTotoStatement::new(
            TRAINING_CORPUS_PREDICATE_TYPE,
            vec![subject],
            serde_json::to_value(&pred).unwrap(),
        );
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, stmt.canonical_json().unwrap().as_bytes()).unwrap();
        path
    }

    fn sample_opts(corpus_path: PathBuf) -> BindOpts {
        BindOpts {
            model: ModelRef {
                identity: "https://huggingface.co/acme/model-m".to_string(),
                weights_manifest_digest: digest_map(3),
                signing_bundle_ref: None,
            },
            model_card_uri: "https://huggingface.co/acme/model-m".to_string(),
            corpora: vec![BoundCorpus {
                bundle_path: corpus_path,
                role: "pretraining".to_string(),
            }],
            builder_identity: "Acme AI".to_string(),
            config_digest: Some(digest_map(4)),
            source_date_epoch: 1_748_476_800,
            builder_version: "attestrum-cli/0.0.1".to_string(),
            sign: false,
            oidc_id_token: None,
            workspace: None,
        }
    }

    #[test]
    fn bind_unsigned_emits_model_binding_statement() {
        let corpus = write_corpus_bundle("attestrum-bind-bind-unsigned-corpus.json", 0xab);
        let opts = sample_opts(corpus.clone());
        let artifact = bind(&opts).unwrap();
        let _ = std::fs::remove_file(&corpus);

        assert!(
            artifact.bundle_path.is_none(),
            "unsigned bind writes no bundle"
        );
        assert_eq!(
            artifact.statement.predicate_type,
            MODEL_BINDING_PREDICATE_TYPE
        );
        artifact.statement.validate().unwrap();

        // The model is the subject; its digest matches the model ref.
        let subj = artifact.statement.subject.first().unwrap();
        assert_eq!(subj.digest, digest_map(3));

        // The corpus material is reflected: digest links, merkle root lifted.
        let pred: ModelBindingPredicate =
            serde_json::from_value(artifact.statement.predicate.clone()).unwrap();
        assert_eq!(pred.corpora.len(), 1);
        assert_eq!(pred.corpora[0].role, "pretraining");
        assert_eq!(pred.corpora[0].merkle_root, "ab".repeat(32));
        assert_eq!(pred.corpora[0].manifest_digest, digest_map(2));
        assert_eq!(pred.training.builder_identity, "Acme AI");
    }

    #[test]
    fn bind_corpus_digest_matches_attestation_digest_of_bundle() {
        // The digest bind() records must equal what walk_chain recomputes from
        // the same bundle file via attestation_digest_of_bundle (Step 2 linkage).
        let corpus = write_corpus_bundle("attestrum-bind-digest-parity-corpus.json", 0xcd);
        let opts = sample_opts(corpus.clone());
        let artifact = bind(&opts).unwrap();
        let from_bundle = attestrum_attest::attestation_digest_of_bundle(&corpus).unwrap();
        let _ = std::fs::remove_file(&corpus);

        let pred: ModelBindingPredicate =
            serde_json::from_value(artifact.statement.predicate.clone()).unwrap();
        assert_eq!(
            pred.corpora[0].attestation_digest, from_bundle,
            "bind()'s recorded digest must equal attestation_digest_of_bundle (walk_chain Step 2)"
        );
    }

    #[test]
    fn bind_signing_without_oidc_token_errors() {
        // The missing-OIDC-when-signing error path (§14 untested-error-path
        // rule). No network: the check precedes any Sigstore call.
        let corpus = write_corpus_bundle("attestrum-bind-missing-oidc-corpus.json", 0xef);
        let mut opts = sample_opts(corpus.clone());
        opts.sign = true;
        opts.oidc_id_token = None;
        let err = bind(&opts).unwrap_err();
        let _ = std::fs::remove_file(&corpus);

        assert!(
            matches!(
                err,
                BindError::Sign(AttestrumAttestError::SigstoreIdentityToken(_))
            ),
            "signing without an OIDC token must error before any network call, got {err:?}"
        );
    }
}
