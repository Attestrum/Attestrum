//! PROTECTED text-MinHash kernel (CLAUDE.md §4) — `normalize_text` +
//! [`minhash::compute`], extracted **byte-identically** from
//! `attestrum-fingerprint` so the same Rust can compile to `wasm32` for the
//! attestrum.com near-match demo (byte-identical by construction, no second
//! implementation to drift). `attestrum-fingerprint` depends on this crate and
//! calls these from its `fingerprint_text` path; the move preserves byte output
//! exactly (P1 spike, 2026-06-06) and changes no public behavior.
//!
//! # PROTECTED
//!
//! Per CLAUDE.md §4 and the E3 landing commit's `Protected-system-change:`
//! footer (founder-approved 2026-05-25, extraction re-approved 2026-06-06): the
//! normalization pipeline and the [`minhash`] algorithm parameters are locked.
//! Any change to NFC / lowercase / whitespace-collapse, shingle size,
//! permutation count, or the BLAKE3 keying scheme invalidates every
//! previously-emitted inclusion proof and requires a schema URI bump.

use unicode_normalization::UnicodeNormalization;

pub mod minhash;

/// PROTECTED text normalization pipeline (CLAUDE.md §4).
///
/// `NFC → str::to_lowercase → split_whitespace + " " join`. Implicit
/// leading / trailing whitespace strip via `split_whitespace`'s skip-empty
/// semantics.
pub fn normalize_text(input: &str) -> String {
    let nfc: String = input.nfc().collect();
    let lower = nfc.to_lowercase();
    lower.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_leading_and_trailing_whitespace() {
        assert_eq!(normalize_text("  hello  "), "hello");
        assert_eq!(normalize_text("\t\nhello\t\n"), "hello");
    }

    #[test]
    fn normalize_collapses_runs_of_whitespace_to_single_ascii_space() {
        assert_eq!(normalize_text("hello\t\n  world"), "hello world");
        assert_eq!(normalize_text("a\u{00A0}b"), "a b"); // NBSP -> space
    }

    #[test]
    fn normalize_lowercases_ascii_and_unicode_scalars() {
        assert_eq!(normalize_text("HELLO"), "hello");
        assert_eq!(normalize_text("HÉLLO"), "héllo");
        // German sharp-S: Unicode-aware lowercase keeps ß (lower-case form).
        assert_eq!(normalize_text("STRASSE"), "strasse");
    }

    #[test]
    fn normalize_nfc_canonicalizes_combining_sequences() {
        // "café" precomposed (U+00E9 for the é).
        let precomposed = "café";
        // "café" decomposed: e + U+0301 (combining acute accent).
        let decomposed = "cafe\u{0301}";
        assert_ne!(precomposed.len(), decomposed.len()); // pre-norm: differ
        assert_eq!(normalize_text(precomposed), normalize_text(decomposed));
    }

    #[test]
    fn normalize_handles_empty_string() {
        assert_eq!(normalize_text(""), "");
        assert_eq!(normalize_text("   \t\n  "), "");
    }
}
