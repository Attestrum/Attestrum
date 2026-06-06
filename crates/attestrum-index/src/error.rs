//! Error type for the fuzzy-lookup index crate.

use thiserror::Error;

/// Errors from building, reading, or querying a fuzzy-lookup sidecar index.
///
/// The reader (`format::FuzzyIndex::from_bytes`) is the source of the
/// validation variants — every one has an exercising test in `format`'s test
/// module per CLAUDE.md §14 (no untested error paths).
#[derive(Debug, Error)]
pub enum IndexError {
    /// File does not start with the `ATSTRMIX` magic.
    #[error("not an attestrum index: bad magic")]
    BadMagic,

    /// Unsupported on-disk format version (only v1 is understood today).
    #[error("unsupported index format version: {0}")]
    UnsupportedVersion(u16),

    /// `SUBINDEX_KIND` header byte is not one of minhash(1)/perceptual(2)/iscc(3).
    #[error("unknown sub-index kind: {0}")]
    UnknownKind(u16),

    /// A signature's `u64` count does not match the declared `SIG_WIDTH`.
    #[error("signature width mismatch: expected {expected}, got {got}")]
    SignatureWidthMismatch {
        /// The header-declared signature width.
        expected: u16,
        /// The width actually supplied for an entry.
        got: usize,
    },

    /// The byte stream ended before a declared field could be fully read.
    #[error("truncated index: needed {needed} more bytes at offset {offset}")]
    Truncated {
        /// Byte offset where the short read occurred.
        offset: usize,
        /// Bytes still required to satisfy the field being read.
        needed: usize,
    },

    /// The trailing BLAKE3 digest does not cover the preceding bytes — the file
    /// is corrupt or was partially written.
    #[error("index trailer hash mismatch: file is corrupt")]
    TrailerMismatch,

    /// Underlying filesystem error while reading or atomically writing.
    #[error("index io error: {0}")]
    Io(#[from] std::io::Error),
}
