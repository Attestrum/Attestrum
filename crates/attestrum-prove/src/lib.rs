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
    AttestrumAttestError, InTotoStatement, InclusionProofPredicate, MatchEvidence,
    NonInclusionProofPredicate, Subject,
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
/// `manifest`. **S5-D2 E1 ships the contract only** — the body is
/// [`unimplemented!`] and calling [`prove`] panics. E2 onward fills it
/// in per the planning diagram at
/// `docs/diagrams/sprint-5/prove-pipeline.md`.
pub fn prove(
    target: ProofTarget,
    manifest: ManifestSource,
    opts: &ProveOpts,
) -> Result<ProofArtifact, AttestrumProveError> {
    let _ = (target, manifest, opts);
    unimplemented!("S5-D2 E2+ fills this in — E1 ships the contract only")
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
