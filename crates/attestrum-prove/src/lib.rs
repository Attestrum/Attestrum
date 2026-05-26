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

pub use attestrum_attest::{
    AttestrumAttestError, CorpusRef, DigestMap, InTotoStatement, InclusionProofPredicate,
    MatchEvidence, NonInclusionProofPredicate, Subject,
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
/// **S5-D2 E2 implements the exact-hash dispatch** ([`ProofTarget::Blake3`],
/// [`ProofTarget::Sha256`], [`ProofTarget::Bundle`]) against
/// [`ManifestSource::Local`] only. The returned [`ProofArtifact`] carries a
/// fully-validating [`InclusionProofPredicate`] wrapped in an
/// [`InTotoStatement`] (predicate type
/// `attestrum.com/attestation/inclusion-proof/v0.3`), with several fields
/// stubbed pending later E-commits:
///
/// - `predicate.audit_path` is `vec![]` (E3 lands the real RFC 6962
///   audit-path via [`attestrum_merkle::MerkleTree::audit_path`]).
/// - `bundle_path` is forced to `None` regardless of `opts.sign` (E4
///   lands DSSE-sign).
/// - `corpus.attestation_digest` is zeros-hex (refined at E4 alongside
///   signing — the digest is of the corpus's signed in-toto Statement
///   which doesn't exist as an E2 input).
/// - `proof_generated_at` / `proof_generator_identity` are `None` (E4
///   populates these from `opts.source_date_epoch` + the OIDC identity).
///
/// Variants not yet implemented panic with clear "S5-D2 E{N}+" messages:
/// the fuzzy paths ([`ProofTarget::Iscc`], [`ProofTarget::Perceptual`],
/// [`ProofTarget::Document`]) land at E5; the non-inclusion path (target
/// absent from manifest) lands at E6; the alternate manifest sources
/// ([`ManifestSource::HuggingFace`], [`ManifestSource::Url`]) land at E7.
pub fn prove(
    target: ProofTarget,
    manifest: ManifestSource,
    opts: &ProveOpts,
) -> Result<ProofArtifact, AttestrumProveError> {
    // E2 reads no `opts` fields directly — `opts.sign` is force-ignored
    // (bundle_path is always None at E2; E4 wires DSSE-sign); the
    // predicate's `proof_generated_at` field would consume
    // `opts.source_date_epoch` at E4 (it stays `None` at E2). The
    // remaining fields (`oidc_id_token`, `workspace`) are E4+/E7+ inputs.
    let _ = opts;

    let manifest_path = match &manifest {
        ManifestSource::Local(p) => p.clone(),
        ManifestSource::HuggingFace { .. } | ManifestSource::Url(_) => {
            unimplemented!("S5-D2 E7 lands HF + URL manifest fetching")
        }
    };

    let (target_b3, target_s256) = match &target {
        ProofTarget::Blake3(b3) => (Some(*b3), None),
        ProofTarget::Sha256(s) => (None, Some(*s)),
        ProofTarget::Bundle(bundle) => {
            let b3 = attestrum_core::hex::decode_32(&bundle.blake3).map_err(|e| {
                AttestrumProveError::InvalidManifest(format!("bundle.blake3 hex: {e}"))
            })?;
            let s = attestrum_core::hex::decode_32(&bundle.sha256).map_err(|e| {
                AttestrumProveError::InvalidManifest(format!("bundle.sha256 hex: {e}"))
            })?;
            (Some(b3), Some(s))
        }
        ProofTarget::Iscc(_) | ProofTarget::Perceptual(_) | ProofTarget::Document(_) => {
            unimplemented!("S5-D2 E5+ lands fuzzy-match paths (ISCC, Perceptual, Document)")
        }
    };

    let entries = attestrum_manifest::read_manifest(&manifest_path)
        .map_err(|e| AttestrumProveError::InvalidManifest(e.to_string()))?;

    let (leaf_index, evidence) = find_exact_match(&entries, target_b3, target_s256)?;
    let entry = &entries[leaf_index];

    let leaves: Vec<[u8; 32]> = entries.iter().map(|e| e.document_id).collect();
    let root = attestrum_merkle::merkle_root(&leaves);

    let matched_subject = entry_to_subject(entry);

    let predicate = InclusionProofPredicate {
        proof_type: InclusionProofPredicate::PROOF_TYPE_VALUE.to_string(),
        corpus: CorpusRef {
            manifest_uri: format!("file://{}", manifest_path.display()),
            merkle_root: attestrum_core::hex::encode_32(&root),
            attestation_digest: DigestMap {
                blake3: "0".repeat(64),
                sha256: "0".repeat(64),
            },
        },
        query_fingerprint: query_fingerprint_json(&target),
        match_evidence: evidence,
        tree_size: entries.len() as u64,
        leaf_count: entries.len() as u64,
        leaf_hash: attestrum_core::hex::encode_32(&entry.document_id),
        hash_algorithm: "blake3-rfc6962".to_string(),
        audit_path: vec![],
        leaf_index: leaf_index as u64,
        matched_subject: matched_subject.clone(),
        proof_generated_at: None,
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

    Ok(ProofArtifact {
        kind: ProofKind::Inclusion,
        statement,
        bundle_path: None,
        confidence: 1.0,
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
) -> Result<(usize, MatchEvidence), AttestrumProveError> {
    if let Some(b3) = target_b3 {
        let hits: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| if e.document_id == b3 { Some(i) } else { None })
            .collect();
        match hits.len() {
            0 => { /* fall through to SHA-256 fallback if Bundle target */ }
            1 => return Ok((hits[0], MatchEvidence::ExactBlake3)),
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
            0 => { /* no match either way → non-inclusion path (E6) */ }
            1 => return Ok((hits[0], MatchEvidence::ExactSha256)),
            n => return Err(AttestrumProveError::Ambiguous(n)),
        }
    }

    unimplemented!("S5-D2 E6 lands non-inclusion path; target not found in manifest")
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
        // The fuzzy variants are unreachable here — `prove()` panics
        // on them via `unimplemented!()` before reaching this helper.
        // Listed for exhaustiveness so the compiler enforces coverage
        // when E5 fills them in.
        ProofTarget::Iscc(_) | ProofTarget::Perceptual(_) | ProofTarget::Document(_) => {
            unreachable!("E5 fills the fuzzy-path query_fingerprint shape")
        }
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
