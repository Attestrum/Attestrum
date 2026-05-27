//! `attestrum-prove` — inclusion / non-inclusion proof builder for the
//! Sprint 5 D2 `attestrum prove` workflow. Consumes the
//! v0.1-frozen [`attestrum_fingerprint`] surface and the v0.3-frozen
//! [`attestrum_attest`] predicate types to emit a signed proof artifact
//! over a corpus manifest.
//!
//! See `docs/diagrams/sprint-5/prove-pipeline.md` for the pipeline diagram
//! covering the full 8-commit E-decomposition (one diagram per deliverable;
//! per-E-commit updates bump its `last_verified` SHA rather than spawning
//! per-commit diagrams).
//!
//! **S5-D2 E1 (this commit) ships the contract only.** The public API
//! surface — the seven items below plus the [`prove`] entry-point — is
//! locked enough to let downstream callers begin integration work. The
//! [`prove`] body is [`unimplemented!`] pending E2 onward; calling it
//! panics with a `"S5-D2 E2+ fills this in"` message. Per CLAUDE.md §14
//! ("Eager generalization") the per-E-commit dep additions are deliberate:
//! `parquet` / `arrow` / `attestrum-manifest` / `attestrum-merkle` land at
//! E2; `hf-hub` / `url` at E7. The public surface freezes at E8 via a
//! hand-rolled `tests/api_surface.rs` golden mirroring the
//! `attestrum-fingerprint` precedent.
//!
//! # E-decomposition (planned)
//!
//! - **E2** — local-Parquet manifest read + exact-BLAKE3 / SHA-256 match
//!   path + [`InclusionProofPredicate`] emission with placeholder
//!   `audit_path: vec![]` and `opts.sign=false` forced. First end-to-end
//!   shape.
//! - **E3** — audit-path via `attestrum_merkle::MerkleTree::audit_path`.
//!   Predicate now carries a real proof.
//! - **E4** — DSSE-sign via [`attestrum_attest::sign`]. **MVP gate**:
//!   first demonstrable signed inclusion proof verifiable via `cosign v3+
//!   verify-blob-attestation --new-bundle-format` end-to-end.
//! - **E5** — fuzzy-match paths ([`ProofTarget::Iscc`],
//!   [`ProofTarget::Perceptual`], MinHash via [`ProofTarget::Document`])
//!   with confidence reporting per PATH-A-BRIEF §2.2 thresholds.
//! - **E6** — [`NonInclusionProofPredicate`] via sorted-Merkle adjacent-
//!   leaves. Requires a PROTECTED-system extension to `attestrum_merkle`
//!   per CLAUDE.md §4 (explicit founder approval in commit footer).
//! - **E7** — alternate manifest sources ([`ManifestSource::HuggingFace`],
//!   [`ManifestSource::Url`]) with caching. May coordinate with Sprint 5
//!   D3 (`attestrum-publish`) on HF Hub auth patterns.
//! - **E8** — `attestrum prove` CLI subcommand + hand-rolled
//!   `tests/api_surface.rs` + `source_of_truth: diagram → code` flip on
//!   the planning diagram. v0.1 release-ready.
//!
//! # PROTECTED dependencies
//!
//! Per CLAUDE.md §4, this crate **consumes but never modifies**:
//!
//! - [`attestrum_attest`] predicate types ([`InclusionProofPredicate`],
//!   [`NonInclusionProofPredicate`], [`MatchEvidence`]) — frozen at the
//!   v0.3 URIs.
//! - `attestrum_merkle::{MerkleTree, audit_path, verify_audit_path}` —
//!   RFC 6962 binary Merkle over BLAKE3 (dep added at E2).
//! - [`attestrum_fingerprint`] — frozen at v0.1 as of S5-D1 E5.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ============================================================================
// S5-D2 E5 — fuzzy-match thresholds + per-mode confidence
// ============================================================================
//
// Hardcoded per PATH-A-BRIEF §2.2 + the planning diagram's design-note #4.
// No exposed knobs at v0.1; a v0.2 `ProveOpts` extension may add caller-
// configurable thresholds.

/// ISCC composite-distance threshold (Hamming bits over the decoded
/// ISCC body). Matches with `composite_distance <= 4` count as hits.
const FUZZY_THRESHOLD_ISCC_DISTANCE: u32 = 4;
/// Perceptual Hamming-distance threshold (bits of difference, max 64).
/// Matches with `hamming_distance <= 6` over either pHash or blockhash
/// count as hits.
const FUZZY_THRESHOLD_PERCEPTUAL_HAMMING: u32 = 6;
/// MinHash Jaccard similarity threshold in parts-per-million.
/// `850_000` ppm == 0.85 similarity. Matches with `jaccard >= 850_000`
/// count as hits.
const FUZZY_THRESHOLD_MINHASH_JACCARD_PPM: u32 = 850_000;

/// Exact-hash match confidence (BLAKE3, SHA-256, Bundle).
const CONFIDENCE_EXACT: f32 = 1.00;
/// ISCC composite-distance match confidence.
const CONFIDENCE_ISCC: f32 = 0.95;
/// Perceptual Hamming-distance match confidence (pHash or blockhash).
const CONFIDENCE_PERCEPTUAL: f32 = 0.85;
/// MinHash Jaccard-similarity match confidence (text-only).
const CONFIDENCE_MINHASH: f32 = 0.80;

pub use attestrum_attest::{
    AttestrumAttestError, CorpusRef, DigestMap, InTotoStatement, InclusionProofPredicate,
    IsccEvidence, MatchEvidence, MinHashEvidence, NonInclusionProofPredicate, PerceptualEvidence,
    Subject,
};
pub use attestrum_fingerprint::{AttestrumFingerprintError, FingerprintBundle};

/// What the caller wants to prove about. Six variants align with the
/// match strategies the [`prove`] dispatcher implements at E2-E5: three
/// exact-hash paths (`Blake3`, `Sha256`, [`ProofTarget::Bundle`] which
/// extracts both from a pre-computed [`FingerprintBundle`]) and three
/// fuzzy paths ([`ProofTarget::Iscc`], [`ProofTarget::Perceptual`],
/// [`ProofTarget::Document`] which runs `fingerprint_text` /
/// `fingerprint_image` inline before dispatching to the multi-mode match
/// pipeline).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofTarget {
    /// 32-byte BLAKE3 digest. Exact-match path against the Merkle leaf
    /// space (which is also BLAKE3-rooted per Sprint 2). Confidence 1.00.
    Blake3([u8; 32]),
    /// 32-byte SHA-256 digest. Exact-match path against the manifest's
    /// SHA-256 column (Sigstore / in-toto interop digest). Confidence
    /// 1.00.
    Sha256([u8; 32]),
    /// ISCC composite string (e.g. `"ISCC:KACT4EBWK27737D2…"`).
    /// Composite-distance fuzzy match. E5 path.
    Iscc(String),
    /// Caller-supplied 64-bit pHash + 64-bit blockhash. Hamming-distance
    /// fuzzy match. E5 path. For the inline path (caller has the raw
    /// document bytes, not the precomputed hashes), use
    /// [`ProofTarget::Document`] instead.
    Perceptual(PerceptualHashes),
    /// Path to a document on disk. The [`prove`] dispatcher runs
    /// `attestrum_fingerprint::fingerprint_text` /
    /// `fingerprint_image` inline, then attempts every match mode
    /// (exact, ISCC, perceptual, MinHash) and returns the
    /// highest-confidence hit. E5 path.
    Document(PathBuf),
    /// Pre-computed [`FingerprintBundle`] from
    /// [`attestrum_fingerprint::fingerprint_text`] /
    /// [`attestrum_fingerprint::fingerprint_image`]. The dispatcher
    /// extracts BLAKE3 + SHA-256 for the exact-match path. E2 path.
    /// Boxed because [`FingerprintBundle`] is dramatically larger than
    /// the hash-array variants — keeps `ProofTarget`'s stack footprint
    /// bounded.
    Bundle(Box<FingerprintBundle>),
}

/// Caller-supplied perceptual hashes for the non-inline match path.
/// Both fields are 64-bit hashes per the v0.1 fingerprint schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptualHashes {
    /// DCT-based pHash (8x8 = 64 bits) as produced by `image_hasher`'s
    /// `HasherConfig::new().hash_size(8, 8).preproc_dct()`.
    pub phash: [u8; 8],
    /// blockhash.io spec 64-bit hash as produced by the `blockhash`
    /// crate.
    pub blockhash: [u8; 8],
}

/// Where to read the corpus manifest from. E2 lands `Local`; E7 lands
/// `HuggingFace` + `Url`.
///
/// `Url` carries a [`String`] at E1 (no URL parsing at this commit);
/// promotion to `url::Url` lands with the E7 fetching code to avoid a
/// phantom dep here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManifestSource {
    /// On-disk Parquet manifest produced by `attestrum build`. mmap'd
    /// via `attestrum_manifest` at E2.
    Local(PathBuf),
    /// Hugging Face dataset repo. Resolved via `hf-hub` at E7. The
    /// `revision` selects a specific commit / tag; `None` resolves to
    /// the default branch (`main`).
    HuggingFace {
        repo: String,
        revision: Option<String>,
    },
    /// Arbitrary HTTPS URL to a Parquet manifest. Fetched + cached at
    /// E7.
    Url(String),
}

/// Caller-tunable knobs for [`prove`]. The defaults match the CLI's
/// defaults at E8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProveOpts {
    /// `true` to DSSE-sign the resulting [`InTotoStatement`] via
    /// [`attestrum_attest::sign`] and populate
    /// [`ProofArtifact::bundle_path`]. `false` for unsigned runs (test
    /// fixtures, dry-runs, CI without OIDC). E4 wires this in; E1-E3
    /// force `false` regardless of caller intent.
    pub sign: bool,
    /// Source-date-epoch (Unix seconds) for deterministic timestamps in
    /// the predicate's `built_at` field and elsewhere. Mirrors the
    /// existing Sprint 3 `--source-date-epoch` convention; same value
    /// across hosts yields byte-identical proof artifacts.
    pub source_date_epoch: i64,
    /// OIDC `id_token` (raw JWT) for Sigstore Fulcio cert issuance. Only
    /// consulted when `sign=true`. The CLI fetches this from
    /// `ATTESTRUM_OIDC_ID_TOKEN` at E8.
    pub oidc_id_token: Option<String>,
    /// Optional override for the workspace dir where downloaded
    /// manifests (E7) and the emitted Bundle JSON (E4) are written.
    /// `None` defaults to `$PWD/.attestrum/prove/` at E8.
    pub workspace: Option<PathBuf>,
    /// Path to the corpus's signed in-toto Statement bundle (Sigstore
    /// Bundle v0.3 JSON, typically produced by `attestrum sign`). When
    /// `Some(_)`, [`prove`] reads and digests the file via
    /// [`attestrum_cas::stream_hash_path`] and populates
    /// `predicate.corpus.attestation_digest` with the real BLAKE3 +
    /// SHA-256 of the bundle bytes. When `None`, `attestation_digest`
    /// stays at the E2 zeros-hex placeholder — the caller hasn't
    /// supplied the corpus's bundle reference, so the field reserves
    /// the schema slot without binding a concrete digest. Added at
    /// S5-D2 E4.
    pub corpus_bundle_path: Option<PathBuf>,
    /// Path to the corpus's CAS root directory (typically
    /// `<corpus_root>/.attestrum/`, the parent of the `cas/blake3/`
    /// subtree). **REQUIRED** when invoking any fuzzy-match
    /// [`ProofTarget`] arm ([`ProofTarget::Iscc`],
    /// [`ProofTarget::Perceptual`], [`ProofTarget::Document`])
    /// because [`prove`] re-fingerprints each manifest leaf on demand
    /// via [`attestrum_cas::CasStore::open`]: there's no precomputed
    /// fuzzy-hash sidecar at v0.1 (a v0.2 optimization may add one).
    /// `None` is fine for the exact-hash arms ([`ProofTarget::Blake3`],
    /// [`ProofTarget::Sha256`], [`ProofTarget::Bundle`]) — those
    /// don't read leaf bytes. Added at S5-D2 E5.
    pub cas_root: Option<PathBuf>,
}

/// The result of a successful [`prove`] call. Either an inclusion or a
/// non-inclusion proof, optionally accompanied by the on-disk path of
/// the DSSE-signed Bundle v0.3 JSON when `opts.sign=true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofArtifact {
    /// Whether the target was found in (`Inclusion`) or definitively
    /// absent from (`NonInclusion`) the manifest.
    pub kind: ProofKind,
    /// The in-toto v1 Statement wrapping the
    /// [`InclusionProofPredicate`] or [`NonInclusionProofPredicate`].
    /// Always present (this is the canonical payload regardless of
    /// signing).
    pub statement: InTotoStatement,
    /// Absolute path of the DSSE-signed Sigstore Bundle v0.3 JSON
    /// written by [`attestrum_attest::sign`]. `Some(_)` only when
    /// `opts.sign=true`; `None` for unsigned runs. Mirrors the
    /// [`attestrum_attest::SignedAttestation::bundle_path`] pattern —
    /// the file is the canonical Bundle, parsed on demand by the
    /// caller. Avoids depending on `sigstore` directly here.
    pub bundle_path: Option<PathBuf>,
    /// Match confidence in `[0.0, 1.0]`. Exact-hash matches are 1.0;
    /// ISCC-composite 0.95; perceptual-Hamming 0.85; MinHash-Jaccard
    /// 0.80; non-inclusion 1.00 (the sorted-Merkle proof is exact). See
    /// PATH-A-BRIEF §2.2 for the per-evidence-kind table.
    pub confidence: f32,
    /// The [`Subject`] (manifest leaf) the proof references. `Some(_)`
    /// for inclusion; `None` for non-inclusion (the target was absent,
    /// so there is no matched subject).
    pub matched_subject: Option<Subject>,
}

/// Discriminator for the two kinds of proof [`prove`] can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofKind {
    /// Target found in the manifest. Statement carries an
    /// [`InclusionProofPredicate`] (predicate type URI
    /// `attestrum.com/attestation/inclusion-proof/v0.3`).
    Inclusion,
    /// Target absent from the manifest. Statement carries a
    /// [`NonInclusionProofPredicate`] (predicate type URI
    /// `attestrum.com/attestation/non-inclusion-proof/v0.3`). E6.
    NonInclusion,
}

/// Crate-wide error kind. Each variant maps to a concrete failure mode
/// in the [`prove`] dispatcher at E2-E7.
#[derive(Debug, thiserror::Error)]
pub enum AttestrumProveError {
    /// Manifest source could not be reached. Network failure on the
    /// `HuggingFace` / `Url` paths (E7); `Local` path-not-found / EACCES
    /// (E2).
    #[error("manifest source unreachable: {0}")]
    SourceUnreachable(String),

    /// Manifest was reachable but its bytes are not a well-formed
    /// Attestrum Parquet manifest (wrong schema, truncated file,
    /// corrupted Zstd blocks, etc.). E2.
    #[error("manifest format invalid: {0}")]
    InvalidManifest(String),

    /// The audit-path's recomputed Merkle root does not match the
    /// manifest's claimed root. Indicates manifest corruption or a
    /// bug in the audit-path layer. E3.
    #[error("merkle root mismatch")]
    MerkleMismatch,

    /// Inline fingerprinting failed on a [`ProofTarget::Document`]
    /// dispatch. Wraps the underlying
    /// [`AttestrumFingerprintError`]. E5.
    #[error("fingerprint failed: {0}")]
    Fingerprint(#[from] AttestrumFingerprintError),

    /// DSSE-sign failed when `opts.sign=true`. Wraps the underlying
    /// [`AttestrumAttestError`] (Fulcio rejection, Rekor submission,
    /// OIDC validation, TUF root refresh, etc.). E4.
    #[error("signing failed: {0}")]
    Sign(#[from] AttestrumAttestError),

    /// The target matched more than one leaf in the manifest. Distinct
    /// from the manifest's `occurrence_index` (which disambiguates
    /// intentional duplicates); this variant indicates the dispatcher
    /// found `N > 1` candidates and the caller must narrow the target.
    /// E2+.
    #[error("ambiguous match: {0} candidates")]
    Ambiguous(usize),
}

/// Build an inclusion or non-inclusion proof for `target` against
/// `manifest`.
///
/// **S5-D2 E2-E4 implement the exact-hash dispatch** ([`ProofTarget::Blake3`],
/// [`ProofTarget::Sha256`], [`ProofTarget::Bundle`]) against
/// [`ManifestSource::Local`] only. The returned [`ProofArtifact`] carries a
/// fully-validating [`InclusionProofPredicate`] wrapped in an
/// [`InTotoStatement`] (predicate type
/// `attestrum.com/attestation/inclusion-proof/v0.3`). As of E3 the
/// predicate is **cryptographically self-contained**: external verifiers
/// can re-derive the corpus's BLAKE3 Merkle root from
/// `predicate.{leaf_hash, leaf_index, tree_size, audit_path}` alone via
/// [`attestrum_merkle::verify_audit_path`], no manifest re-read required.
///
/// **E4 wires DSSE-sign (the MVP gate).** When `opts.sign=true`, the
/// canonicalized Statement is signed via [`attestrum_attest::sign`] —
/// Sigstore Bundle v0.3, DSSE envelope, Fulcio ephemeral cert, Rekor v1
/// `dsse@0.0.1` transparency entry — and the resulting bundle is
/// written to `<opts.workspace.or($PWD/.attestrum)>/prove/inclusion-
/// proof.sigstore.json`. The path is echoed back in
/// `ProofArtifact.bundle_path`. The bundle verifies end-to-end via
/// `cosign v3+ verify-blob-attestation --new-bundle-format` without
/// Attestrum installed. When `opts.sign=false`, behavior matches E3
/// exactly: `bundle_path: None`, no network, no OIDC.
///
/// `opts.oidc_id_token` is required when `opts.sign=true`; `None` in
/// that combination surfaces as
/// [`AttestrumProveError::Sign`] wrapping
/// [`AttestrumAttestError::SigstoreIdentityToken`].
///
/// **E4 also populates two additional predicate fields** unconditionally
/// (signed or unsigned):
/// - `proof_generated_at` is derived from `opts.source_date_epoch` via
///   [`jiff::Timestamp::from_second`] (RFC 3339, deterministic).
/// - `corpus.attestation_digest` is populated when
///   `opts.corpus_bundle_path = Some(_)` (BLAKE3 + SHA-256 via
///   [`attestrum_cas::stream_hash_path`]); otherwise stays at the E2
///   zeros-hex placeholder.
///
/// One field still stubbed (deferred to a hypothetical v0.4 schema bump):
///
/// - `proof_generator_identity` is `None` even when signed. The bundle's
///   leaf cert is the authoritative identity binding; populating the
///   predicate field would require pre-sign JWT parsing (new dep,
///   redundant with the cert) or a circular two-pass sign scheme.
///   Verifiers cross-check identity against the bundle via
///   [`attestrum_attest::identity::extract_identity`].
///
/// **S5-D2 E5 closes the remaining `ProofTarget` arms** — the three
/// fuzzy paths ([`ProofTarget::Iscc`], [`ProofTarget::Perceptual`],
/// [`ProofTarget::Document`]) — via **CAS re-fingerprint at prove time**:
/// when invoked, [`prove`] opens `opts.cas_root` as a
/// [`attestrum_cas::CasStore`], iterates the manifest leaves, fetches
/// each leaf's bytes via [`attestrum_cas::CasStore::open`], runs
/// `fingerprint_text` / `fingerprint_image` per `entry.modality`, and
/// computes the relevant distance vs the caller's target. No
/// precomputed fuzzy-hash sidecar exists at v0.1 — a v0.2 optimization
/// may add one. `opts.cas_root` is REQUIRED for any fuzzy dispatch
/// (returns `InvalidManifest` when missing). `Modality::{Audio, Video,
/// Pdf, Other}` leaves are silently skipped during fuzzy scans
/// (`attestrum-fingerprint` v0.1 supports only Text + Image).
///
/// Variants not yet implemented panic with clear "S5-D2 E{N}+"
/// messages: zero-match outcomes (would-be non-inclusion) panic
/// pending E6; alternate manifest sources
/// ([`ManifestSource::HuggingFace`], [`ManifestSource::Url`]) panic
/// pending E7.
pub fn prove(
    target: ProofTarget,
    manifest: ManifestSource,
    opts: &ProveOpts,
) -> Result<ProofArtifact, AttestrumProveError> {
    let manifest_path = match &manifest {
        ManifestSource::Local(p) => p.clone(),
        ManifestSource::HuggingFace { .. } | ManifestSource::Url(_) => {
            unimplemented!("S5-D2 E7 lands HF + URL manifest fetching")
        }
    };

    let entries = attestrum_manifest::read_manifest(&manifest_path)
        .map_err(|e| AttestrumProveError::InvalidManifest(e.to_string()))?;

    let (leaf_index, evidence, confidence) = match &target {
        ProofTarget::Blake3(_) | ProofTarget::Sha256(_) | ProofTarget::Bundle(_) => {
            let (target_b3, target_s256) = extract_exact_targets(&target)?;
            match find_exact_match(&entries, target_b3, target_s256)? {
                Some((idx, evi)) => (idx, evi, CONFIDENCE_EXACT),
                None => unimplemented!(
                    "S5-D2 E6 lands non-inclusion path; target not found in manifest"
                ),
            }
        }
        ProofTarget::Iscc(iscc_str) => dispatch_iscc(iscc_str, &entries, opts)?,
        ProofTarget::Perceptual(hashes) => dispatch_perceptual(hashes, &entries, opts)?,
        ProofTarget::Document(path) => dispatch_document(path, &entries, opts)?,
    };
    let entry = &entries[leaf_index];

    let tree = attestrum_merkle::MerkleTree::new(entries.iter().map(|e| e.document_id).collect());
    let root = tree.root();
    let audit = tree.audit_path(leaf_index).unwrap_or_else(|e| {
        // Unreachable by construction: find_exact_match returns a leaf_index
        // bounded by entries.len(), and tree was built from the same entries.
        // audit_path only errors with IndexOutOfBounds when leaf_index >=
        // tree_size, which can't happen here.
        unreachable!(
            "find_exact_match returned in-bounds leaf_index={leaf_index} \
             for tree of size {} but audit_path rejected it: {e}",
            tree.len()
        )
    });

    let matched_subject = entry_to_subject(entry);

    let attestation_digest = match &opts.corpus_bundle_path {
        Some(p) => {
            let h = attestrum_cas::stream_hash_path(p).map_err(|e| {
                AttestrumProveError::InvalidManifest(format!("corpus_bundle_path read: {e}"))
            })?;
            DigestMap {
                blake3: attestrum_core::hex::encode_32(&h.blake3),
                sha256: attestrum_core::hex::encode_32(&h.sha256),
            }
        }
        None => DigestMap {
            blake3: "0".repeat(64),
            sha256: "0".repeat(64),
        },
    };

    let proof_generated_at = jiff::Timestamp::from_second(opts.source_date_epoch)
        .map_err(|_| {
            AttestrumProveError::InvalidManifest(format!(
                "invalid source_date_epoch: {}",
                opts.source_date_epoch
            ))
        })?
        .to_string();

    let predicate = InclusionProofPredicate {
        proof_type: InclusionProofPredicate::PROOF_TYPE_VALUE.to_string(),
        corpus: CorpusRef {
            manifest_uri: format!("file://{}", manifest_path.display()),
            merkle_root: attestrum_core::hex::encode_32(&root),
            attestation_digest,
        },
        query_fingerprint: query_fingerprint_json(&target),
        match_evidence: evidence,
        tree_size: entries.len() as u64,
        leaf_count: entries.len() as u64,
        leaf_hash: attestrum_core::hex::encode_32(&entry.document_id),
        hash_algorithm: "blake3-rfc6962".to_string(),
        audit_path: audit.iter().map(attestrum_core::hex::encode_32).collect(),
        leaf_index: leaf_index as u64,
        matched_subject: matched_subject.clone(),
        proof_generated_at: Some(proof_generated_at),
        proof_generator_identity: None,
    };
    predicate.validate()?;

    let predicate_json = serde_json::to_value(&predicate)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("predicate serialize: {e}")))?;
    let statement = InTotoStatement::new(
        attestrum_attest::INCLUSION_PROOF_PREDICATE_TYPE,
        vec![matched_subject.clone()],
        predicate_json,
    );

    let bundle_path = if opts.sign {
        let oidc_token = opts.oidc_id_token.clone().ok_or_else(|| {
            AttestrumProveError::Sign(AttestrumAttestError::SigstoreIdentityToken(
                "ProveOpts.oidc_id_token must be Some when ProveOpts.sign is true".into(),
            ))
        })?;
        let canonical = statement.canonical_json().map_err(|e| {
            AttestrumProveError::InvalidManifest(format!("statement canonical_json: {e}"))
        })?;
        let workspace_dir = opts.workspace.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.join(".attestrum"))
                .unwrap_or_else(|_| PathBuf::from(".attestrum"))
        });
        let bundle_dir = workspace_dir.join("prove");
        std::fs::create_dir_all(&bundle_dir).map_err(|e| {
            AttestrumProveError::InvalidManifest(format!("create workspace prove dir: {e}"))
        })?;
        let bundle_out = bundle_dir.join("inclusion-proof.sigstore.json");
        let signed = attestrum_attest::sign(attestrum_attest::SignRequest {
            statement_payload: canonical.as_bytes(),
            bundle_output_path: &bundle_out,
            oidc_id_token: oidc_token,
        })?;
        Some(signed.bundle_path)
    } else {
        None
    };

    Ok(ProofArtifact {
        kind: ProofKind::Inclusion,
        statement,
        bundle_path,
        confidence,
        matched_subject: Some(matched_subject),
    })
}

/// Scan `entries` for a leaf whose `document_id` matches `target_b3`
/// (preferred) or whose `sha256` matches `target_s256` (fallback for
/// SHA-256-only and Bundle targets when no BLAKE3 hit exists).
///
/// Returns the leaf index + the corresponding [`MatchEvidence`] variant.
/// More than one hit on the same column returns
/// [`AttestrumProveError::Ambiguous`] with the candidate count — the
/// manifest's multiset policy permits duplicate `document_id`s, so the
/// caller must disambiguate via a future `target_occurrence_index`
/// option (E5+).
///
/// **E2 panics on zero hits.** The non-inclusion path (target absent
/// from manifest) lands at E6 via [`NonInclusionProofPredicate`] over
/// the sorted-Merkle adjacent-leaves technique.
fn find_exact_match(
    entries: &[attestrum_manifest::ManifestEntry],
    target_b3: Option<[u8; 32]>,
    target_s256: Option<[u8; 32]>,
) -> Result<Option<(usize, MatchEvidence)>, AttestrumProveError> {
    if let Some(b3) = target_b3 {
        let hits: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| if e.document_id == b3 { Some(i) } else { None })
            .collect();
        match hits.len() {
            0 => { /* fall through to SHA-256 fallback if Bundle target */ }
            1 => return Ok(Some((hits[0], MatchEvidence::ExactBlake3))),
            n => return Err(AttestrumProveError::Ambiguous(n)),
        }
    }

    if let Some(s256) = target_s256 {
        let hits: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| if e.sha256 == s256 { Some(i) } else { None })
            .collect();
        match hits.len() {
            0 => { /* no match either way → caller (prove() exact-arm or
                 dispatch_document) decides: panic with E6 message
                 or fall through to fuzzy mode. */
            }
            1 => return Ok(Some((hits[0], MatchEvidence::ExactSha256))),
            n => return Err(AttestrumProveError::Ambiguous(n)),
        }
    }

    Ok(None)
}

/// Build a [`Subject`] from a manifest leaf. `name` is taken from
/// `source_url` when present, falling back to a deterministic
/// `"doc-<input_ordinal>"` synthetic identifier.
fn entry_to_subject(entry: &attestrum_manifest::ManifestEntry) -> Subject {
    let name = entry
        .source_url
        .clone()
        .unwrap_or_else(|| format!("doc-{}", entry.input_ordinal));
    Subject {
        name,
        digest: DigestMap {
            blake3: attestrum_core::hex::encode_32(&entry.document_id),
            sha256: attestrum_core::hex::encode_32(&entry.sha256),
        },
    }
}

/// Build the predicate's `query_fingerprint` JSON Value describing the
/// caller's input target. Includes only the hash form(s) the caller
/// supplied; future E5+ extensions add ISCC / Perceptual / MinHash
/// query fields when the fuzzy paths land.
fn query_fingerprint_json(target: &ProofTarget) -> serde_json::Value {
    match target {
        ProofTarget::Blake3(b3) => serde_json::json!({
            "blake3": attestrum_core::hex::encode_32(b3),
        }),
        ProofTarget::Sha256(s) => serde_json::json!({
            "sha256": attestrum_core::hex::encode_32(s),
        }),
        ProofTarget::Bundle(bundle) => serde_json::json!({
            "blake3": bundle.blake3,
            "sha256": bundle.sha256,
        }),
        ProofTarget::Iscc(iscc) => serde_json::json!({ "iscc": iscc }),
        ProofTarget::Perceptual(p) => serde_json::json!({
            "phash": encode_hex_8(&p.phash),
            "blockhash": encode_hex_8(&p.blockhash),
        }),
        ProofTarget::Document(path) => serde_json::json!({
            "documentPath": path.to_string_lossy(),
        }),
    }
}

// ============================================================================
// S5-D2 E5 — exact-target extraction helper (factored out of the dispatch
// match so the four arms share a uniform `(idx, evi, confidence)` shape).
// ============================================================================

/// `(blake3, sha256)` exact-hash target pair extracted from a
/// `ProofTarget`. `None` on either side means "no target for that
/// column" (e.g., `ProofTarget::Blake3` carries Some BLAKE3, None
/// SHA-256). Type alias suppresses clippy::type_complexity on the
/// `extract_exact_targets` return.
type ExactTargets = (Option<[u8; 32]>, Option<[u8; 32]>);

/// Extract `(target_b3, target_s256)` from the three exact-hash arms of
/// [`ProofTarget`]. Errors on hex-parse failure for the Bundle arm.
/// Returns `(None, None)` for fuzzy arms, which the caller must never
/// reach.
fn extract_exact_targets(target: &ProofTarget) -> Result<ExactTargets, AttestrumProveError> {
    match target {
        ProofTarget::Blake3(b3) => Ok((Some(*b3), None)),
        ProofTarget::Sha256(s) => Ok((None, Some(*s))),
        ProofTarget::Bundle(bundle) => {
            let b3 = attestrum_core::hex::decode_32(&bundle.blake3).map_err(|e| {
                AttestrumProveError::InvalidManifest(format!("bundle.blake3 hex: {e}"))
            })?;
            let s = attestrum_core::hex::decode_32(&bundle.sha256).map_err(|e| {
                AttestrumProveError::InvalidManifest(format!("bundle.sha256 hex: {e}"))
            })?;
            Ok((Some(b3), Some(s)))
        }
        ProofTarget::Iscc(_) | ProofTarget::Perceptual(_) | ProofTarget::Document(_) => {
            unreachable!("extract_exact_targets called on fuzzy variant — dispatcher bug")
        }
    }
}

// ============================================================================
// S5-D2 E5 — distance helpers (hand-rolled; ~5 LOC each).
// ============================================================================
//
// Kept private to `attestrum-prove` rather than adding to PROTECTED
// `attestrum-fingerprint`. Stateless arithmetic; not load-bearing
// crate-public surface.

/// ISCC composite distance: Hamming bits over the body bytes of two
/// decoded ISCC strings. Both inputs must decode to bodies of equal
/// length (a length mismatch indicates the two ISCC codes have
/// different MainType / SubType / length headers and aren't
/// distance-comparable).
fn iscc_composite_distance(a: &str, b: &str) -> Result<u32, AttestrumProveError> {
    let (_, _, _, _, a_body) = iscc_lib::iscc_decode(a)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("iscc decode target: {e}")))?;
    let (_, _, _, _, b_body) = iscc_lib::iscc_decode(b)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("iscc decode leaf: {e}")))?;
    if a_body.len() != b_body.len() {
        return Err(AttestrumProveError::InvalidManifest(format!(
            "iscc body length mismatch: {} vs {}",
            a_body.len(),
            b_body.len()
        )));
    }
    Ok(a_body
        .iter()
        .zip(b_body.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum())
}

/// Decode a 16-char lowercase hex string into the 8-byte hash it
/// represents (used for pHash + blockhash). Errors on length / hex
/// parse violations.
fn decode_hex_8(s: &str) -> Result<[u8; 8], AttestrumProveError> {
    if s.len() != 16 {
        return Err(AttestrumProveError::InvalidManifest(format!(
            "expected 16-char hex, got {} chars",
            s.len()
        )));
    }
    let mut out = [0u8; 8];
    for (i, byte) in out.iter_mut().enumerate() {
        let hex_byte = &s[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(hex_byte, 16)
            .map_err(|e| AttestrumProveError::InvalidManifest(format!("hex parse: {e}")))?;
    }
    Ok(out)
}

/// Lowercase-hex encode an 8-byte hash (the inverse of [`decode_hex_8`]).
fn encode_hex_8(bytes: &[u8; 8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(16);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// 64-bit Hamming distance over two 16-char lowercase hex strings.
/// Returns `0..=64`.
fn hamming_distance_hex64(a: &str, b: &str) -> Result<u32, AttestrumProveError> {
    let a_b = decode_hex_8(a)?;
    let b_b = decode_hex_8(b)?;
    Ok(a_b
        .iter()
        .zip(b_b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum())
}

/// MinHash Jaccard similarity in parts-per-million (0..=1_000_000).
/// Both slices must be of length 128 (PROTECTED `attestrum-fingerprint`
/// v0.1 lock).
fn minhash_jaccard_ppm(a: &[u64], b: &[u64]) -> u32 {
    debug_assert_eq!(a.len(), 128, "MinHash v0.1 schema is 128 perms");
    debug_assert_eq!(b.len(), 128, "MinHash v0.1 schema is 128 perms");
    let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    ((matches as u64) * 1_000_000 / 128) as u32
}

// ============================================================================
// S5-D2 E5 — fuzzy-match dispatchers.
//
// Each iterates `entries`, opens the corpus CAS via opts.cas_root,
// fetches per-leaf bytes by `entry.document_id`, fingerprints per
// `entry.modality`, computes distance vs target, and returns the
// best-match `(leaf_index, MatchEvidence, confidence)`. Zero-match
// outcomes panic with the E6 message (non-inclusion is deferred).
// Modality::{Audio,Video,Pdf,Other} leaves are silently skipped
// (fingerprint crate supports only Text + Image at v0.1).
// ============================================================================

/// Open the corpus's CAS or surface a clear "cas_root required" error.
fn open_cas(opts: &ProveOpts) -> Result<attestrum_cas::CasStore, AttestrumProveError> {
    let root = opts.cas_root.as_ref().ok_or_else(|| {
        AttestrumProveError::InvalidManifest(
            "cas_root required for fuzzy-match dispatch (ISCC / Perceptual / Document)".into(),
        )
    })?;
    attestrum_cas::CasStore::new(root)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("open cas: {e}")))
}

/// Fetch one leaf's bytes from the CAS by its BLAKE3 document_id.
fn read_leaf_bytes(
    cas: &attestrum_cas::CasStore,
    document_id: &[u8; 32],
) -> Result<Vec<u8>, AttestrumProveError> {
    use std::io::Read;
    let mut file = cas
        .open(document_id)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("cas open leaf: {e}")))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("cas read leaf: {e}")))?;
    Ok(bytes)
}

/// Fingerprint one leaf's bytes per its declared modality. Returns
/// `Ok(None)` for unsupported modalities (Audio / Video / Pdf / Other)
/// so the caller can silently skip; returns `Ok(Some(bundle))` for
/// Text / Image; returns `Err(_)` only on real fingerprint failure
/// (e.g. invalid UTF-8 in a text-marked leaf, undecodable image bytes).
fn fingerprint_leaf(
    bytes: &[u8],
    modality: attestrum_fingerprint::Modality,
    source_date_epoch: i64,
) -> Result<Option<FingerprintBundle>, AttestrumProveError> {
    let fp_opts = attestrum_fingerprint::FingerprintOpts { source_date_epoch };
    match modality {
        attestrum_fingerprint::Modality::Text => Ok(Some(attestrum_fingerprint::fingerprint_text(
            bytes, &fp_opts,
        )?)),
        attestrum_fingerprint::Modality::Image => Ok(Some(
            attestrum_fingerprint::fingerprint_image(bytes, &fp_opts)?,
        )),
        _ => Ok(None),
    }
}

fn dispatch_iscc(
    target_iscc: &str,
    entries: &[attestrum_manifest::ManifestEntry],
    opts: &ProveOpts,
) -> Result<(usize, MatchEvidence, f32), AttestrumProveError> {
    let cas = open_cas(opts)?;
    let mut best: Option<(usize, u32)> = None;
    for (idx, entry) in entries.iter().enumerate() {
        let bytes = read_leaf_bytes(&cas, &entry.document_id)?;
        let bundle = match fingerprint_leaf(&bytes, entry.modality, opts.source_date_epoch)? {
            Some(b) => b,
            None => continue,
        };
        let leaf_iscc = match bundle.iscc.as_ref() {
            Some(iscc) => &iscc.composite,
            None => continue,
        };
        let dist = iscc_composite_distance(target_iscc, leaf_iscc)?;
        if dist <= FUZZY_THRESHOLD_ISCC_DISTANCE && best.map(|(_, d)| dist < d).unwrap_or(true) {
            best = Some((idx, dist));
        }
    }
    match best {
        Some((idx, dist)) => Ok((
            idx,
            MatchEvidence::Iscc(attestrum_attest::IsccEvidence {
                composite_distance: dist,
            }),
            CONFIDENCE_ISCC,
        )),
        None => unimplemented!(
            "S5-D2 E6 lands non-inclusion path; ISCC target had no leaf with composite_distance <= {}",
            FUZZY_THRESHOLD_ISCC_DISTANCE
        ),
    }
}

fn dispatch_perceptual(
    target: &PerceptualHashes,
    entries: &[attestrum_manifest::ManifestEntry],
    opts: &ProveOpts,
) -> Result<(usize, MatchEvidence, f32), AttestrumProveError> {
    let cas = open_cas(opts)?;
    let mut best: Option<(usize, u32)> = None;
    for (idx, entry) in entries.iter().enumerate() {
        if !matches!(entry.modality, attestrum_fingerprint::Modality::Image) {
            continue;
        }
        let bytes = read_leaf_bytes(&cas, &entry.document_id)?;
        let bundle = match fingerprint_leaf(&bytes, entry.modality, opts.source_date_epoch)? {
            Some(b) => b,
            None => continue,
        };
        let image = match bundle.image.as_ref() {
            Some(i) => i,
            None => continue,
        };
        let dist_phash = hamming_distance_hex64(&encode_hex_8(&target.phash), &image.phash)?;
        let dist_blockhash =
            hamming_distance_hex64(&encode_hex_8(&target.blockhash), &image.blockhash)?;
        let leaf_min = dist_phash.min(dist_blockhash);
        if leaf_min <= FUZZY_THRESHOLD_PERCEPTUAL_HAMMING
            && best.map(|(_, d)| leaf_min < d).unwrap_or(true)
        {
            best = Some((idx, leaf_min));
        }
    }
    match best {
        Some((idx, dist)) => Ok((
            idx,
            MatchEvidence::Perceptual(attestrum_attest::PerceptualEvidence {
                hamming_distance: dist,
                threshold: FUZZY_THRESHOLD_PERCEPTUAL_HAMMING,
            }),
            CONFIDENCE_PERCEPTUAL,
        )),
        None => unimplemented!(
            "S5-D2 E6 lands non-inclusion path; Perceptual target had no image leaf within hamming <= {}",
            FUZZY_THRESHOLD_PERCEPTUAL_HAMMING
        ),
    }
}

fn dispatch_document(
    path: &std::path::Path,
    entries: &[attestrum_manifest::ManifestEntry],
    opts: &ProveOpts,
) -> Result<(usize, MatchEvidence, f32), AttestrumProveError> {
    let bytes = std::fs::read(path)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("document read: {e}")))?;
    let fp_opts = attestrum_fingerprint::FingerprintOpts {
        source_date_epoch: opts.source_date_epoch,
    };
    let (input_modality, input_bundle) = if std::str::from_utf8(&bytes).is_ok() {
        let bundle = attestrum_fingerprint::fingerprint_text(&bytes, &fp_opts)?;
        (attestrum_fingerprint::Modality::Text, bundle)
    } else if image::guess_format(&bytes).is_ok() {
        let bundle = attestrum_fingerprint::fingerprint_image(&bytes, &fp_opts)?;
        (attestrum_fingerprint::Modality::Image, bundle)
    } else {
        return Err(AttestrumProveError::InvalidManifest(format!(
            "unsupported document modality at {path:?}: not UTF-8 and not a recognized image format"
        )));
    };

    // 1. Try exact BLAKE3 / SHA-256 first (highest confidence, fastest —
    //    no CAS scan). Note: this works for image Documents (where both
    //    BLAKE3 inputs are raw bytes), but NOT for text Documents
    //    (fingerprint_text hashes the normalized bytes per the PROTECTED
    //    v0.1 pipeline; the manifest stores raw-bytes BLAKE3 via
    //    stream_hash). Text Documents reliably fall through to the
    //    ISCC / MinHash fuzzy fallbacks below.
    let input_b3 = attestrum_core::hex::decode_32(&input_bundle.blake3)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("document blake3 hex: {e}")))?;
    let input_s256 = attestrum_core::hex::decode_32(&input_bundle.sha256)
        .map_err(|e| AttestrumProveError::InvalidManifest(format!("document sha256 hex: {e}")))?;
    if let Some((idx, evi)) = find_exact_match(entries, Some(input_b3), Some(input_s256))? {
        return Ok((idx, evi, CONFIDENCE_EXACT));
    }

    // 2. Fall back to the fuzzy modes in confidence order (ISCC > Perceptual > MinHash).
    if let Some(iscc) = input_bundle.iscc.as_ref() {
        if let Ok(hit) = dispatch_iscc(&iscc.composite, entries, opts) {
            return Ok(hit);
        }
    }
    if matches!(input_modality, attestrum_fingerprint::Modality::Image) {
        if let Some(image) = input_bundle.image.as_ref() {
            let p = PerceptualHashes {
                phash: decode_hex_8(&image.phash)?,
                blockhash: decode_hex_8(&image.blockhash)?,
            };
            if let Ok(hit) = dispatch_perceptual(&p, entries, opts) {
                return Ok(hit);
            }
        }
    }
    if matches!(input_modality, attestrum_fingerprint::Modality::Text) {
        if let Some(text) = input_bundle.text.as_ref() {
            if let Ok(hit) = dispatch_minhash(&text.minhash, entries, opts) {
                return Ok(hit);
            }
        }
    }

    unimplemented!("S5-D2 E6 lands non-inclusion path; Document target had no match in any mode")
}

/// MinHash dispatcher — only invoked from `dispatch_document` for the
/// text-modality path (MinHash isn't exposed as its own `ProofTarget`
/// variant; per PATH-A-BRIEF §2.2 it's reached via Document).
fn dispatch_minhash(
    target_minhash: &[u64],
    entries: &[attestrum_manifest::ManifestEntry],
    opts: &ProveOpts,
) -> Result<(usize, MatchEvidence, f32), AttestrumProveError> {
    let cas = open_cas(opts)?;
    let mut best: Option<(usize, u32)> = None;
    for (idx, entry) in entries.iter().enumerate() {
        if !matches!(entry.modality, attestrum_fingerprint::Modality::Text) {
            continue;
        }
        let bytes = read_leaf_bytes(&cas, &entry.document_id)?;
        let bundle = match fingerprint_leaf(&bytes, entry.modality, opts.source_date_epoch)? {
            Some(b) => b,
            None => continue,
        };
        let text = match bundle.text.as_ref() {
            Some(t) => t,
            None => continue,
        };
        let jaccard = minhash_jaccard_ppm(target_minhash, &text.minhash);
        if jaccard >= FUZZY_THRESHOLD_MINHASH_JACCARD_PPM
            && best.map(|(_, j)| jaccard > j).unwrap_or(true)
        {
            best = Some((idx, jaccard));
        }
    }
    match best {
        Some((idx, jaccard)) => Ok((
            idx,
            MatchEvidence::MinHash(attestrum_attest::MinHashEvidence {
                jaccard,
                ngram_size: 5,
            }),
            CONFIDENCE_MINHASH,
        )),
        None => unimplemented!(
            "S5-D2 E6 lands non-inclusion path; MinHash target had no text leaf within jaccard >= {} ppm",
            FUZZY_THRESHOLD_MINHASH_JACCARD_PPM
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_target_variants_construct() {
        let _ = ProofTarget::Blake3([0u8; 32]);
        let _ = ProofTarget::Sha256([0u8; 32]);
        let _ = ProofTarget::Iscc(String::from("ISCC:KACT4EBWK27737D2"));
        let _ = ProofTarget::Perceptual(PerceptualHashes {
            phash: [0u8; 8],
            blockhash: [0u8; 8],
        });
        let _ = ProofTarget::Document(PathBuf::from("/dev/null"));
        let bundle = FingerprintBundle {
            schema: String::from("https://attestrum.com/fingerprint/v0.1"),
            modality: attestrum_fingerprint::Modality::Text,
            blake3: String::from("00").repeat(32),
            sha256: String::from("00").repeat(32),
            byte_len: 0,
            text: None,
            image: None,
            iscc: None,
            generated_at: String::from("1970-01-01T00:00:00Z"),
        };
        let _ = ProofTarget::Bundle(Box::new(bundle));
    }

    #[test]
    fn manifest_source_variants_construct() {
        let _ = ManifestSource::Local(PathBuf::from("/dev/null"));
        let _ = ManifestSource::HuggingFace {
            repo: String::from("allenai/c4"),
            revision: Some(String::from("refs/convert/parquet")),
        };
        let _ = ManifestSource::HuggingFace {
            repo: String::from("allenai/c4"),
            revision: None,
        };
        let _ = ManifestSource::Url(String::from("https://example.com/manifest.parquet"));
    }

    #[test]
    fn prove_opts_constructs() {
        let opts = ProveOpts {
            sign: false,
            source_date_epoch: 0,
            oidc_id_token: None,
            workspace: None,
            corpus_bundle_path: None,
            cas_root: None,
        };
        assert!(!opts.sign);
        assert_eq!(opts.source_date_epoch, 0);
    }

    #[test]
    fn proof_kind_variants_construct() {
        let k = ProofKind::Inclusion;
        let nk = ProofKind::NonInclusion;
        assert_ne!(k, nk);
    }

    #[test]
    fn error_debug_round_trip() {
        let errs = [
            AttestrumProveError::SourceUnreachable(String::from("connection refused")),
            AttestrumProveError::InvalidManifest(String::from("bad magic")),
            AttestrumProveError::MerkleMismatch,
            AttestrumProveError::Ambiguous(3),
        ];
        for e in &errs {
            let s = format!("{e:?}");
            assert!(!s.is_empty());
            let d = format!("{e}");
            assert!(!d.is_empty());
        }
    }

    #[test]
    fn perceptual_hashes_is_copy() {
        let p = PerceptualHashes {
            phash: [1u8; 8],
            blockhash: [2u8; 8],
        };
        let q = p;
        assert_eq!(p, q);
    }
}
