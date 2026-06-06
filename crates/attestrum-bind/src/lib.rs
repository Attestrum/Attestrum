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

use std::path::{Path, PathBuf};

use attestrum_attest::{
    attestation_digest_of_bundle, attestation_digest_of_statement, statement_from_bundle,
    verify as attest_verify, verify_statement, AttestrumAttestError, CorpusBindingRef, DigestMap,
    InTotoStatement, ModelBindingPredicate, ModelRef, SignRequest, Subject,
    TrainingCorpusPredicate, TrainingMeta, VerifyRequest, MODEL_BINDING_PREDICATE_TYPE,
};
use attestrum_prove::{
    prove, AttestrumProveError, ManifestSource, ProofArtifact, ProofKind, ProofTarget, ProveOpts,
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

// ============================================================================
// walk_chain — the signed keystone verification
// ============================================================================

/// Sigstore identity policy applied to a bundle during the walk (cosign-shaped
/// anchored regexes + an offline flag). Reused for both the binding and corpus
/// bundles.
#[derive(Debug, Clone, Copy)]
pub struct IdentityPolicy<'a> {
    /// Anchored regex matched against the bundle's extracted SAN.
    pub identity_regex: &'a str,
    /// Anchored regex matched against the bundle's Fulcio OIDC-issuer extension.
    pub issuer_regex: &'a str,
    /// Skip the online Rekor inclusion re-check (TUF refresh may still network).
    pub offline: bool,
}

/// The model-binding side of the chain: a signed `model-binding/v0.1` bundle,
/// the model weights-manifest file it attests (verify() re-reads it and asserts
/// SHA-256 == the binding subject digest), and the identity policy.
pub struct BindingInput<'a> {
    /// Signed Sigstore Bundle v0.3 carrying the model-binding Statement.
    pub bundle_path: &'a Path,
    /// The model weights-manifest file — the binding bundle's in-toto subject.
    pub manifest_path: &'a Path,
    /// Identity policy applied to the binding bundle.
    pub policy: IdentityPolicy<'a>,
}

/// The corpus side: a signed `training-corpus/v0.3` bundle, the **local** corpus
/// manifest (Parquet) file it attests, and the identity policy. The membership
/// step re-runs `prove()` against this manifest. Local-only at v0.1: verify()
/// needs the manifest bytes on disk to check the subject digest, so a remote
/// (HF/URL) corpus manifest must be materialized locally first.
pub struct CorpusInput<'a> {
    /// Signed Sigstore Bundle v0.3 carrying the training-corpus Statement.
    pub bundle_path: &'a Path,
    /// The local corpus manifest (Parquet) — the corpus bundle's in-toto subject
    /// AND the manifest `prove()` re-runs against.
    pub manifest_path: &'a Path,
    /// Identity policy applied to the corpus bundle.
    pub policy: IdentityPolicy<'a>,
}

/// Outcome of a successful chain walk against one bound corpus.
#[derive(Debug)]
pub enum ChainWalkOutcome {
    /// Work X is in this corpus, and this corpus trained the model.
    InCorpus {
        /// The corpus's role (e.g. `"pretraining"`).
        role: String,
        /// The (live-recomputed) inclusion proof.
        proof: ProofArtifact,
    },
    /// Work X is definitively absent from this corpus (which trained the model).
    NotInCorpus {
        /// The (live-recomputed) non-inclusion proof.
        proof: ProofArtifact,
    },
}

/// Walk the chain **model digest → signed binding → signed corpus attestation →
/// live membership proof of work X**, answering "is X in the corpus that trained
/// M?".
///
/// **Signed-only (D3-A).** Both the binding and corpus bundles are
/// Sigstore-verified (signature + identity policy) before any field is read, so
/// there is no public path that claims "the chain walks" without cryptographic
/// verification. All field reads come from the *verified* statements.
///
/// For a multi-corpus model, call once per bound corpus and OR the `InCorpus`
/// results ("X in at least one corpus that trained M"). `query` must be an exact
/// [`ProofTarget`] arm (`Blake3`/`Sha256`/`Bundle`) at v0.1 — the fuzzy arms
/// require a CAS root, which this entry point does not thread through.
///
/// **Honest ceiling (D6):** the membership step RE-RUNS `prove()` live against
/// the verifier-supplied manifest — it **manufactures** the inclusion proof
/// rather than verifying an independently-signed one. Membership is therefore
/// only as strong as the manifest fed to `prove()`; a dishonest trainer who
/// attests a sanitized corpus is not caught here. See
/// `docs/diagrams/binding/model-binding-and-chain-walk.md`.
pub fn walk_chain(
    model_digest: &DigestMap,
    binding: BindingInput<'_>,
    corpus: CorpusInput<'_>,
    query: ProofTarget,
) -> Result<ChainWalkOutcome, ChainWalkError> {
    // Verify-first: both bundles' Sigstore signatures + identity policy. The
    // binding must be a model-binding/v0.1 bundle; the corpus a training-corpus
    // bundle (verify() pins training-corpus). Every field read below comes from
    // the VERIFIED statements — never a fresh re-parse of the raw file (TOCTOU).
    let v_bind = verify_statement(VerifyRequest {
        bundle_path: binding.bundle_path,
        manifest_path: binding.manifest_path,
        identity_regex: binding.policy.identity_regex,
        issuer_regex: binding.policy.issuer_regex,
        offline: binding.policy.offline,
        expected_predicate_type: Some(MODEL_BINDING_PREDICATE_TYPE),
    })
    .map_err(ChainWalkError::BindingVerify)?;

    let v_corp = attest_verify(VerifyRequest {
        bundle_path: corpus.bundle_path,
        manifest_path: corpus.manifest_path,
        identity_regex: corpus.policy.identity_regex,
        issuer_regex: corpus.policy.issuer_regex,
        offline: corpus.policy.offline,
        expected_predicate_type: None, // defaults to training-corpus
    })
    .map_err(ChainWalkError::CorpusVerify)?;

    walk_membership(
        model_digest,
        &v_bind.statement,
        &v_corp.predicate.merkle_root,
        corpus.bundle_path,
        ManifestSource::Local(corpus.manifest_path.to_path_buf()),
        query,
    )
}

/// Steps 1–3 of the walk, operating on already-verified statements (or, in the
/// `#[cfg(test)]` fixtures, raw unsigned ones). Separated so the signed public
/// API and the unsigned soundness tests share one core — there is no public
/// unsigned entry point.
///
/// Step 3 re-runs `prove()` live (D6, the honest-ceiling step documented on
/// [`walk_chain`]).
fn walk_membership(
    model_digest: &DigestMap,
    binding_statement: &InTotoStatement,
    corpus_merkle_root: &str,
    corpus_bundle_path: &Path,
    manifest_source: ManifestSource,
    query: ProofTarget,
) -> Result<ChainWalkOutcome, ChainWalkError> {
    let binding_pred: ModelBindingPredicate =
        serde_json::from_value(binding_statement.predicate.clone())
            .map_err(ChainWalkError::BindingPredicate)?;

    // Step 1 — model identity: the binding's subject digest is the model digest.
    let subject_digest = binding_statement
        .subject
        .first()
        .map(|s| &s.digest)
        .ok_or(ChainWalkError::NoSubject)?;
    if subject_digest != model_digest {
        return Err(ChainWalkError::ModelIdentityMismatch {
            expected: Box::new(model_digest.clone()),
            actual: Box::new(subject_digest.clone()),
        });
    }

    // Step 2 — corpus linkage. Recompute the corpus attestation digest
    // canonically from the bundle FILE (does NOT trust prove()'s emitted field
    // — D6 independence) and find the matching bound corpus.
    let computed_digest =
        attestation_digest_of_bundle(corpus_bundle_path).map_err(ChainWalkError::Corpus)?;
    let bound: &CorpusBindingRef = binding_pred
        .corpora
        .iter()
        .find(|c| c.attestation_digest == computed_digest)
        .ok_or_else(|| ChainWalkError::CorpusNotBound {
            computed: Box::new(computed_digest.clone()),
        })?;
    if corpus_merkle_root != bound.merkle_root {
        return Err(ChainWalkError::MerkleRootMismatch {
            binding: bound.merkle_root.clone(),
            corpus: corpus_merkle_root.to_string(),
        });
    }

    // Step 3 — membership. Re-run prove() (unsigned) against the corpus manifest
    // (D6). Passing corpus_bundle_path feeds prove()'s canonical
    // attestationDigest, but note: because Step 2 already located `bound` BY that
    // same canonical digest of the same bundle file, the proof is bound to the
    // verified corpus *by construction* — the signed single-bundle design
    // prevents a proof-against-a-different-corpus rather than detecting it (the
    // spike's separate `ProofCorpusMismatch` check is subsumed by `CorpusNotBound`
    // at Step 2). See the verification report.
    let opts = ProveOpts {
        sign: false,
        source_date_epoch: binding_pred.training.source_date_epoch,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: Some(corpus_bundle_path.to_path_buf()),
        cas_root: None,
        no_index: false,
    };
    let proof = prove(query, manifest_source, &opts).map_err(ChainWalkError::Prove)?;

    match proof.kind {
        ProofKind::Inclusion => Ok(ChainWalkOutcome::InCorpus {
            role: bound.role.clone(),
            proof,
        }),
        ProofKind::NonInclusion => Ok(ChainWalkOutcome::NotInCorpus { proof }),
    }
}

/// Errors from [`walk_chain`]. Each variant is the failure of a specific chain
/// link, so a test (or a verifier) can assert *where* a forged input broke.
#[derive(Debug, thiserror::Error)]
pub enum ChainWalkError {
    /// The binding bundle failed Sigstore verification or identity policy.
    #[error("binding bundle verification failed: {0}")]
    BindingVerify(AttestrumAttestError),
    /// The corpus bundle failed Sigstore verification or identity policy.
    #[error("corpus bundle verification failed: {0}")]
    CorpusVerify(AttestrumAttestError),
    /// The binding predicate is not a model-binding/v0.1 payload.
    #[error("binding predicate is not a model-binding/v0.1 payload: {0}")]
    BindingPredicate(serde_json::Error),
    /// The binding statement carries no subject.
    #[error("binding statement has no subject")]
    NoSubject,
    /// Step 1: the binding subject's digest is not the model digest.
    #[error("model identity mismatch: expected {expected:?}, binding subject {actual:?}")]
    ModelIdentityMismatch {
        /// The model digest the caller asserted.
        expected: Box<DigestMap>,
        /// The digest the binding's subject actually carries.
        actual: Box<DigestMap>,
    },
    /// Recomputing the corpus attestation digest from the bundle file failed.
    #[error("recomputing corpus attestation digest failed: {0}")]
    Corpus(AttestrumAttestError),
    /// Step 2: the supplied corpus attestation is not among the bound corpora.
    #[error("supplied corpus attestation (digest {computed:?}) is not bound to this model")]
    CorpusNotBound {
        /// The digest computed from the supplied corpus bundle.
        computed: Box<DigestMap>,
    },
    /// Step 2: the corpus's claimed Merkle root differs from the binding's.
    #[error("merkle root mismatch: binding {binding}, corpus {corpus}")]
    MerkleRootMismatch {
        /// Root recorded in the binding.
        binding: String,
        /// Root in the verified corpus statement.
        corpus: String,
    },
    /// Step 3: `prove` itself failed.
    #[error("prove failed: {0}")]
    Prove(AttestrumProveError),
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

#[cfg(test)]
mod walk_tests {
    //! Default-suite soundness for the chain walk. These exercise the private
    //! `walk_membership` core (steps 1–3) over **real manifests + RFC 6962
    //! BLAKE3 roots** but UNSIGNED inputs — there is no public unsigned entry
    //! point (D3-A). The signed crypto layer (BindingVerify / CorpusVerify) is
    //! covered by the `#[ignore]` OIDC test in `tests/chain_walk_signed.rs`.
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use attestrum_attest::{
        DeterminismFields, LicensingPosture, ManifestRef, RulesetMode, SignalCoverage,
        TRAINING_CORPUS_PREDICATE_TYPE,
    };
    use attestrum_core::Modality;
    use attestrum_manifest::{
        assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest,
        ManifestEntry, ManifestSignals,
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const EPOCH: i64 = 1_780_012_800; // 2026-05-29T00:00:00Z

    fn fresh_dir(name: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("attestrum-bind-walk-{name}-{n}"));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn db(b: u8) -> [u8; 32] {
        [b; 32]
    }
    fn dmap(b: u8) -> DigestMap {
        DigestMap {
            blake3: attestrum_core::hex::encode_32(&db(b)),
            sha256: attestrum_core::hex::encode_32(&db(b ^ 0xff)),
        }
    }

    fn sample_entry(doc: u8) -> ManifestEntry {
        ManifestEntry {
            document_id: db(doc),
            sha256: db(doc ^ 0xff),
            size_bytes: u64::from(doc) * 100,
            modality: Modality::Text,
            mime_type: Some("text/plain".into()),
            source_url: Some(format!("file:///docs/doc-{doc:02x}.txt")),
            source_type: None,
            source_dataset_id: None,
            registered_domain: None,
            license_spdx: None,
            language: None,
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
            chunk_refs: None,
            input_ordinal: 0,
            occurrence_index: 0,
        }
    }

    fn build_manifest(dir: &Path, name: &str, docs: &[u8]) -> PathBuf {
        let mut entries: Vec<ManifestEntry> = docs.iter().map(|b| sample_entry(*b)).collect();
        assign_input_ordinals(&mut entries);
        sort_entries(&mut entries);
        assign_occurrence_indices(&mut entries);
        let path = dir.join(format!("{name}.parquet"));
        write_manifest(&path, &entries).unwrap();
        path
    }

    /// Training-corpus attestation whose `merkle_root` is the REAL RFC 6962
    /// BLAKE3 root over the manifest. `homepage` differentiates two otherwise
    /// identical corpora so their canonical bytes (and attestation digests)
    /// differ.
    fn corpus_statement(manifest: &Path, homepage: &str) -> InTotoStatement {
        let entries = attestrum_manifest::read_manifest(manifest).unwrap();
        let leaves: Vec<[u8; 32]> = entries.iter().map(|e| e.document_id).collect();
        let root_hex = attestrum_core::hex::encode_32(&attestrum_merkle::merkle_root(&leaves));
        let byte_count: u64 = entries.iter().map(|e| e.size_bytes).sum();
        let pred = TrainingCorpusPredicate {
            attestrum_version: "v0.0.1".to_string(),
            builder_version: "attestrum-cli/0.0.1".to_string(),
            built_at: "2026-05-29T00:00:00Z".to_string(),
            determinism: DeterminismFields {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                seed: EPOCH.to_string(),
                manifest_schema_version: "1".to_string(),
            },
            manifest: ManifestRef {
                uri: format!("attestrum://corpus/{homepage}/manifest.parquet"),
                digest_set: dmap(0xab),
                row_count: entries.len() as u64,
                byte_count,
            },
            merkle_root: root_hex,
            merkle_algorithm: "blake3-rfc6962".to_string(),
            ruleset_mode: RulesetMode::Strict,
            ruleset_id: "attestrum-default".to_string(),
            ruleset_version: "v0.1.0".to_string(),
            signal_coverage: SignalCoverage::default(),
            licensing_posture: LicensingPosture::AllOpenLicensed,
            license_inventory: vec![],
            takedown_contact: None,
            dataset_homepage: Some(homepage.to_string()),
            publication_intent: None,
            total_compute: None,
            training_cost: None,
            model_name: None,
        };
        let subject = Subject {
            name: "manifest.parquet".to_string(),
            digest: dmap(0xcd),
        };
        InTotoStatement::new(
            TRAINING_CORPUS_PREDICATE_TYPE,
            vec![subject],
            serde_json::to_value(&pred).unwrap(),
        )
    }

    fn write_bundle(dir: &Path, name: &str, stmt: &InTotoStatement) -> PathBuf {
        let path = dir.join(format!("{name}.intoto.json"));
        std::fs::write(&path, stmt.canonical_json().unwrap().as_bytes()).unwrap();
        path
    }

    fn model_ref(b: u8, signing: Option<String>) -> ModelRef {
        ModelRef {
            identity: "https://huggingface.co/acme/model-m".to_string(),
            weights_manifest_digest: dmap(b),
            signing_bundle_ref: signing,
        }
    }

    fn merkle_of(stmt: &InTotoStatement) -> String {
        let p: TrainingCorpusPredicate = serde_json::from_value(stmt.predicate.clone()).unwrap();
        p.merkle_root
    }

    /// Build an unsigned binding over the given (bundle_path, role) corpora.
    fn bind_unsigned(corpora: Vec<(&Path, &str)>, model: ModelRef) -> InTotoStatement {
        let opts = BindOpts {
            model_card_uri: model.identity.clone(),
            model,
            corpora: corpora
                .into_iter()
                .map(|(p, r)| BoundCorpus {
                    bundle_path: p.to_path_buf(),
                    role: r.to_string(),
                })
                .collect(),
            builder_identity: "Acme AI".to_string(),
            config_digest: None,
            source_date_epoch: EPOCH,
            builder_version: "attestrum-cli/0.0.1".to_string(),
            sign: false,
            oidc_id_token: None,
            workspace: None,
        };
        bind(&opts).unwrap().statement
    }

    #[test]
    fn in_corpus_work_walks_to_inclusion() {
        let dir = fresh_dir("in_corpus");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        let outcome = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .expect("walk");
        match outcome {
            ChainWalkOutcome::InCorpus { role, proof } => {
                assert_eq!(role, "pretraining");
                assert_eq!(proof.kind, ProofKind::Inclusion);
            }
            other => panic!("expected InCorpus, got {other:?}"),
        }
    }

    #[test]
    fn out_of_corpus_work_walks_to_non_inclusion() {
        let dir = fresh_dir("out_corpus");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        let outcome = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(0x88)),
        )
        .expect("walk");
        assert!(matches!(outcome, ChainWalkOutcome::NotInCorpus { .. }));
    }

    #[test]
    fn multi_corpus_membership_in_at_least_one() {
        let dir = fresh_dir("multi");
        let manifest_a = build_manifest(&dir, "corpusA", &[1, 2, 3]);
        let manifest_b = build_manifest(&dir, "corpusB", &[10, 11, 12]);
        let corpus_a = corpus_statement(&manifest_a, "corpusA");
        let corpus_b = corpus_statement(&manifest_b, "corpusB");
        let bundle_a = write_bundle(&dir, "corpusA", &corpus_a);
        let bundle_b = write_bundle(&dir, "corpusB", &corpus_b);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(
            vec![(&bundle_a, "pretraining"), (&bundle_b, "finetuning")],
            model.clone(),
        );

        // doc 11 is in corpusB only.
        let a = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus_a),
            &bundle_a,
            ManifestSource::Local(manifest_a),
            ProofTarget::Blake3(db(11)),
        )
        .unwrap();
        let b = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus_b),
            &bundle_b,
            ManifestSource::Local(manifest_b),
            ProofTarget::Blake3(db(11)),
        )
        .unwrap();
        assert!(matches!(a, ChainWalkOutcome::NotInCorpus { .. }));
        assert!(
            matches!(b, ChainWalkOutcome::InCorpus { ref role, .. } if role == "finetuning"),
            "doc 11 is in corpusB — 'in at least one corpus that trained M'"
        );
    }

    #[test]
    fn wrong_model_digest_breaks_at_step1() {
        let dir = fresh_dir("wrong_model");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model_ref(0x42, None));

        let err = walk_membership(
            &dmap(0x99), // a different model digest than the binding subject
            &binding,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(
            matches!(err, ChainWalkError::ModelIdentityMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn unbound_corpus_breaks_at_step2() {
        let dir = fresh_dir("unbound");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus_a = corpus_statement(&manifest, "corpusA");
        let bundle_a = write_bundle(&dir, "corpusA", &corpus_a);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle_a, "pretraining")], model.clone());

        // A different corpus bundle (different homepage → different digest) that
        // the model was never bound to.
        let corpus_b = corpus_statement(&manifest, "corpusB-SWAPPED");
        let bundle_b = write_bundle(&dir, "corpusB", &corpus_b);

        let err = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus_b),
            &bundle_b,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(
            matches!(err, ChainWalkError::CorpusNotBound { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn tampered_merkle_root_breaks_at_step2() {
        let dir = fresh_dir("merkle");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        // The corpus bundle IS bound (digest matches), but the verified corpus's
        // claimed merkle root differs from what the binding recorded.
        let err = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &"ff".repeat(32), // wrong merkle root
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(
            matches!(err, ChainWalkError::MerkleRootMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn non_binding_predicate_errors() {
        let dir = fresh_dir("nonbinding");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);

        // Pass a TRAINING-CORPUS statement where a binding is expected.
        let err = walk_membership(
            &dmap(0x42),
            &corpus,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(
            matches!(err, ChainWalkError::BindingPredicate(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn binding_without_subject_errors() {
        let dir = fresh_dir("nosubject");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        // A binding Statement with the same predicate but NO subject.
        let pred = binding.predicate.clone();
        let no_subject = InTotoStatement::new(MODEL_BINDING_PREDICATE_TYPE, vec![], pred);

        let err = walk_membership(
            &model.weights_manifest_digest,
            &no_subject,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(matches!(err, ChainWalkError::NoSubject), "got {err:?}");
    }

    #[test]
    fn prove_failure_surfaces_as_prove_error() {
        let dir = fresh_dir("provefail");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let model = model_ref(0x42, None);
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        // A non-existent manifest makes prove() fail at step 3.
        let err = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(dir.join("does-not-exist.parquet")),
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(matches!(err, ChainWalkError::Prove(_)), "got {err:?}");
    }

    #[test]
    fn signing_bundle_ref_round_trips_and_walks() {
        let dir = fresh_dir("openssf");
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);
        let corpus = corpus_statement(&manifest, "corpusA");
        let bundle = write_bundle(&dir, "corpus", &corpus);
        let signing = "https://huggingface.co/acme/model-m/resolve/main/model.sig".to_string();
        let model = model_ref(0x42, Some(signing.clone()));
        let binding = bind_unsigned(vec![(&bundle, "pretraining")], model.clone());

        let pred: ModelBindingPredicate =
            serde_json::from_value(binding.predicate.clone()).unwrap();
        assert_eq!(
            pred.model.signing_bundle_ref.as_deref(),
            Some(signing.as_str())
        );

        let outcome = walk_membership(
            &model.weights_manifest_digest,
            &binding,
            &merkle_of(&corpus),
            &bundle,
            ManifestSource::Local(manifest),
            ProofTarget::Blake3(db(1)),
        )
        .unwrap();
        assert!(matches!(outcome, ChainWalkOutcome::InCorpus { .. }));
    }

    #[test]
    fn walk_chain_rejects_unverifiable_binding_bundle() {
        // The public signed entry point: an unverifiable binding bundle is
        // rejected at the crypto layer (offline, no network) as BindingVerify.
        let dir = fresh_dir("bindingverify");
        let junk = dir.join("not-a-bundle.json");
        std::fs::write(&junk, b"{\"not\":\"a bundle\"}").unwrap();
        let manifest = build_manifest(&dir, "corpus", &[1, 2, 3]);

        let policy = IdentityPolicy {
            identity_regex: ".*",
            issuer_regex: ".*",
            offline: true,
        };
        let err = walk_chain(
            &dmap(0x42),
            BindingInput {
                bundle_path: &junk,
                manifest_path: &junk,
                policy,
            },
            CorpusInput {
                bundle_path: &junk,
                manifest_path: &manifest,
                policy,
            },
            ProofTarget::Blake3(db(2)),
        )
        .unwrap_err();
        assert!(
            matches!(err, ChainWalkError::BindingVerify(_)),
            "got {err:?}"
        );
    }
}
