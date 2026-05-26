//! `attestrum-core` — shared types, errors, and helpers used by every other Attestrum crate.
//!
//! Sprint 1 commit E7 ships the minimal foundation: [`AttestrumError`], [`Modality`],
//! [`DocumentDigest`], [`BuildContext`], and an in-tree [`hex`] module. No I/O,
//! no network, no async. Every type is `Serialize + Deserialize` so downstream
//! crates can pass them through JSON / Parquet / RocksDB without re-implementing.
//!
//! See `docs/diagrams/sprint-1/attestrum-core-types.md` for the class diagram. The
//! diagram is the contract this code implements; drift between the two is a
//! build break per CLAUDE.md §2.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod hex;

// ============================================================================
// Errors
// ============================================================================

/// Project-wide error kind. Per CLAUDE.md §14, every `Result::Err` branch has
/// at least one test that exercises it.
#[derive(Error, Debug)]
pub enum AttestrumError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("signal parse error: {0}")]
    Signal(String),

    #[error("hash error: {0}")]
    Hash(String),

    #[error("internal: {0}")]
    Internal(String),
}

/// Convenience alias used across Attestrum crates.
pub type Result<T> = std::result::Result<T, AttestrumError>;

// ============================================================================
// Modality
// ============================================================================

/// Document modality — the broad content kind. Mirrors PATH-A-BRIEF §2.1's
/// `Fingerprinter::modality` return type so `attestrum-fingerprint` re-uses this
/// enum verbatim in Sprint 5.
///
/// Derives `schemars::JsonSchema` as of Sprint 5 S5-D1 E5 so the canonical
/// `FingerprintBundle` JSON Schema (published at
/// `attestrum.com/fingerprint/v0.1.schema.json`) can resolve the `modality`
/// field's enum shape without a remote-derive shim. `schemars` is a workspace
/// dep already pulled in by `attestrum-attest`; adding it here is a graph
/// promotion, not a new external dep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
    Other,
}

impl Modality {
    /// Map a MIME type (e.g., `text/plain; charset=utf-8`) to a `Modality`.
    /// Unknown types map to [`Modality::Other`].
    pub fn from_mime(mime: &str) -> Self {
        let lower = mime.to_ascii_lowercase();
        // Strip a `; charset=...` suffix if present.
        let bare = lower.split(';').next().unwrap_or(&lower).trim();
        match bare {
            "application/json" | "application/xml" | "application/yaml" => Modality::Text,
            "application/pdf" => Modality::Pdf,
            _ if bare.starts_with("text/") => Modality::Text,
            _ if bare.starts_with("image/") => Modality::Image,
            _ if bare.starts_with("audio/") => Modality::Audio,
            _ if bare.starts_with("video/") => Modality::Video,
            _ => Modality::Other,
        }
    }

    /// Map a file extension (without the leading `.`) to a `Modality`.
    pub fn from_extension(ext: &str) -> Self {
        let lower = ext.to_ascii_lowercase();
        match lower.as_str() {
            "txt" | "md" | "html" | "htm" | "json" | "xml" | "csv" | "tsv" | "yaml" | "yml"
            | "toml" => Modality::Text,
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "tiff" | "bmp" | "svg" => Modality::Image,
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "opus" => Modality::Audio,
            "mp4" | "mkv" | "webm" | "mov" | "avi" => Modality::Video,
            "pdf" => Modality::Pdf,
            _ => Modality::Other,
        }
    }
}

// ============================================================================
// SourceType
// ============================================================================

/// Provenance classification of a corpus document. Mirrors BUILD-PLAN §4.2's
/// `source_type` dictionary column and §7's `pub enum SourceType` module-boundary
/// sketch. The manifest crate consumes this alongside [`Modality`] to characterize
/// each manifest row's origin.
///
/// Sprint 3 E2 ships the enum + serde roundtrip; Parquet dictionary encoding
/// of these variants lands at E3 with the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceType {
    /// Crawled from the open web (subject to robots.txt / ai.txt / TDMRep / etc.).
    Crawl,
    /// Pulled from a published openly-licensed dataset (Common Pile,
    /// FineWeb-Edu, Dolma, etc.).
    PublicDataset,
    /// Acquired under a private commercial license.
    PrivateLicensed,
    /// User-supplied (uploaded by the corpus operator).
    User,
    /// Synthetic / model-generated.
    Synthetic,
    /// Unclassified or unknown.
    Other,
}

// ============================================================================
// DocumentDigest
// ============================================================================

/// Dual-hash digest carried by every document. Both BLAKE3 (Attestrum-native — used
/// by the Merkle tree and CAS) and SHA-256 (Sigstore/in-toto interop, since
/// `subject[].digest.sha256` is mandatory in DSSE envelopes) are stored.
///
/// Sprint 1 only defines the type. Stream-hashing implementation lands in
/// Sprint 2 (`attestrum-cas` write path, BUILD-PLAN §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentDigest {
    pub blake3: [u8; 32],
    pub sha256: [u8; 32],
}

impl DocumentDigest {
    /// Render as `"blake3:<hex> sha256:<hex>"` for human display and log lines.
    pub fn to_hex(&self) -> String {
        format!(
            "blake3:{} sha256:{}",
            hex::encode_32(&self.blake3),
            hex::encode_32(&self.sha256),
        )
    }

    /// Parse from a pair of hex strings. Each must be exactly 64 hex chars.
    pub fn from_hex_pair(blake3_hex: &str, sha256_hex: &str) -> Result<Self> {
        let blake3 = hex::decode_32(blake3_hex)?;
        let sha256 = hex::decode_32(sha256_hex)?;
        Ok(Self { blake3, sha256 })
    }
}

// ============================================================================
// BuildContext
// ============================================================================

/// Runtime context for a single `attestrum build` / `attestrum prove` invocation.
/// Carries the workspace root + the reproducible-builds timestamp
/// (`SOURCE_DATE_EPOCH` per BUILD-PLAN §6.5).
///
/// CAS path resolution is intentionally NOT here — it belongs in `attestrum-cas`
/// (per CLAUDE.md §4 protected-system isolation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildContext {
    pub workspace_root: PathBuf,
    pub source_date_epoch: i64,
}

impl BuildContext {
    pub fn new(workspace_root: PathBuf, source_date_epoch: i64) -> Self {
        Self {
            workspace_root,
            source_date_epoch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modality_from_mime_text() {
        assert_eq!(Modality::from_mime("text/plain"), Modality::Text);
        assert_eq!(
            Modality::from_mime("text/html; charset=utf-8"),
            Modality::Text
        );
        assert_eq!(Modality::from_mime("application/json"), Modality::Text);
    }

    #[test]
    fn modality_from_mime_image_audio_video_pdf() {
        assert_eq!(Modality::from_mime("image/png"), Modality::Image);
        assert_eq!(Modality::from_mime("IMAGE/JPEG"), Modality::Image);
        assert_eq!(Modality::from_mime("audio/mp3"), Modality::Audio);
        assert_eq!(Modality::from_mime("video/mp4"), Modality::Video);
        assert_eq!(Modality::from_mime("application/pdf"), Modality::Pdf);
    }

    #[test]
    fn modality_from_mime_unknown() {
        assert_eq!(
            Modality::from_mime("application/octet-stream"),
            Modality::Other
        );
        assert_eq!(Modality::from_mime("weirdness"), Modality::Other);
    }

    #[test]
    fn modality_from_extension() {
        assert_eq!(Modality::from_extension("md"), Modality::Text);
        assert_eq!(Modality::from_extension("MP4"), Modality::Video);
        assert_eq!(Modality::from_extension("pdf"), Modality::Pdf);
        assert_eq!(Modality::from_extension("unknown"), Modality::Other);
    }

    #[test]
    fn document_digest_to_hex() {
        let d = DocumentDigest {
            blake3: [1; 32],
            sha256: [2; 32],
        };
        let s = d.to_hex();
        assert!(s.starts_with("blake3:0101"));
        assert!(s.contains(" sha256:0202"));
    }

    #[test]
    fn document_digest_from_hex_pair_roundtrip() {
        let original = DocumentDigest {
            blake3: [0xab; 32],
            sha256: [0xcd; 32],
        };
        let b3_hex = hex::encode_32(&original.blake3);
        let s_hex = hex::encode_32(&original.sha256);
        let restored = DocumentDigest::from_hex_pair(&b3_hex, &s_hex).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn document_digest_rejects_wrong_length() {
        assert!(DocumentDigest::from_hex_pair("aa", "bb").is_err());
    }

    #[test]
    fn build_context_new() {
        let ctx = BuildContext::new(PathBuf::from("/tmp/foo"), 1_234_567_890);
        assert_eq!(ctx.source_date_epoch, 1_234_567_890);
        assert_eq!(ctx.workspace_root, PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn build_context_round_trips_via_serde_json() {
        let ctx = BuildContext::new(PathBuf::from("/tmp/foo"), 100);
        let json = serde_json::to_string(&ctx).unwrap();
        let back: BuildContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source_date_epoch, 100);
        assert_eq!(back.workspace_root, PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn attestrum_error_display_messages() {
        let e = AttestrumError::Config("bad value".into());
        assert!(e.to_string().contains("config error"));
        assert!(e.to_string().contains("bad value"));

        let e2 = AttestrumError::Hash("odd length".into());
        assert!(e2.to_string().contains("hash error"));
    }

    #[test]
    fn attestrum_error_io_from_conversion() {
        let io_err = std::io::Error::other("test");
        let e: AttestrumError = io_err.into();
        match e {
            AttestrumError::Io(_) => {}
            other => panic!("expected Io variant, got {other:?}"),
        }
    }

    #[test]
    fn source_type_round_trips_via_serde_json() {
        for variant in [
            SourceType::Crawl,
            SourceType::PublicDataset,
            SourceType::PrivateLicensed,
            SourceType::User,
            SourceType::Synthetic,
            SourceType::Other,
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            let back: SourceType = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn source_type_variants_are_distinct_under_serde() {
        let crawl = serde_json::to_string(&SourceType::Crawl).unwrap();
        let public_dataset = serde_json::to_string(&SourceType::PublicDataset).unwrap();
        let other = serde_json::to_string(&SourceType::Other).unwrap();
        assert_ne!(crawl, public_dataset);
        assert_ne!(crawl, other);
        assert_ne!(public_dataset, other);
    }
}
