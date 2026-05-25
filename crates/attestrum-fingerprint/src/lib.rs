//! `attestrum-fingerprint` — deterministic per-document fingerprints for the
//! Sprint 5 [`attestrum prove`] workflow. Pure functions; no I/O beyond the
//! caller-supplied input bytes; no system-clock reads (cross-target byte
//! determinism per CLAUDE.md §7).
//!
//! See `docs/diagrams/sprint-5/fingerprint-pipeline.md` for the pipeline
//! diagram (the single diagram covering all of S5-D1; per-E-commit updates
//! bump its `last_verified` SHA rather than spawning per-commit diagrams).
//!
//! Sprint 5 E1 (this commit) ships the text branch only:
//! [`fingerprint_text`] applies the **PROTECTED** normalization pipeline
//! (NFC → `str::to_lowercase` → whitespace collapse) and produces a
//! [`FingerprintBundle`] carrying both BLAKE3 (Attestrum-native, Merkle leaf
//! input) and SHA-256 (Sigstore / in-toto interop) digests of the normalized
//! bytes. Subsequent commits in S5-D1 add the image branch (E2), MinHash +
//! SimHash (E3), and ISCC composition (E4); the API freeze + cross-target
//! determinism gate lands at E5.
//!
//! # PROTECTED
//!
//! Per CLAUDE.md §4: once any inclusion proof is emitted citing the
//! `attestrum.com/fingerprint/v0.1` schema URI, the text-normalization
//! pipeline is immutable. Changing [`normalize_text`] in any future commit
//! invalidates every previously-emitted inclusion proof and requires both a
//! `Protected-system-change:` commit-message footer AND a schema URI bump
//! from `…/fingerprint/v0.1` → `…/fingerprint/v0.2` with a migration packet.
//!
//! # Modality reuse
//!
//! [`Modality`] is re-exported from [`attestrum_core`] — there is no
//! second `Modality` enum in the workspace. The 6-variant project-wide enum
//! covers Text / Image / Audio / Video / Pdf / Other; Sprint 5's narrower
//! implementation scope is expressed by which `fingerprint_*` entry-points
//! this crate exposes (Sprint 5 ships `fingerprint_text` at E1 + image /
//! bytes at E2; Audio / Video / Pdf inputs route through the bytes path or
//! surface as [`AttestrumFingerprintError::ModalityNotImplemented`] from a
//! future dispatch entry).

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub use attestrum_core::Modality;

/// JSON-LD `$id` of the [`FingerprintBundle`] schema. **Frozen at v0.1 as of
/// Sprint 5 E1** — bumping this URI is part of a PROTECTED-system-change
/// (see crate-level docs).
pub const FINGERPRINT_SCHEMA: &str = "https://attestrum.com/fingerprint/v0.1";

// ============================================================================
// FingerprintBundle
// ============================================================================

/// Deterministic fingerprint of a single document. Sprint 5 E1 lands the
/// text-only shape: `blake3` + `sha256` + `byte_len` + `text` populated for
/// text inputs; `image` + `iscc` fields land in E2/E4 as non-breaking
/// optional-field additions.
///
/// Serialization is canonical-JSON-friendly (`camelCase` keys, sorted via
/// `attestrum-attest`'s `deterministic_json_vec` at the call site — this
/// crate produces ordered field output via `serde_json::to_value` /
/// `serde_json::to_string`; the caller is responsible for byte-deterministic
/// re-emission when embedding the bundle in a signed predicate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintBundle {
    /// Schema URI. Always [`FINGERPRINT_SCHEMA`] for E1-emitted bundles.
    pub schema: String,
    /// Which modality was fingerprinted. For E1 this is always
    /// [`Modality::Text`]; image / bytes paths arrive at E2.
    pub modality: Modality,
    /// BLAKE3-256 over the **normalized** bytes (post-normalization for
    /// text; raw input bytes for image / Other when those branches land).
    /// Hex-encoded lowercase, 64 chars.
    pub blake3: String,
    /// SHA-256 over the **normalized** bytes. Same input as `blake3`.
    /// Hex-encoded lowercase, 64 chars. Mandatory (in-toto Subject DSSE
    /// envelopes require `digest.sha256`; we always populate both for
    /// downstream interop).
    pub sha256: String,
    /// Byte length of the bytes that were hashed (post-normalization for
    /// text; raw input length for image / Other branches).
    pub byte_len: u64,
    /// Text-specific details. Present iff `modality == Modality::Text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextFingerprint>,
    /// Deterministic timestamp (RFC 3339). Set from
    /// `FingerprintOpts.source_date_epoch` — never the system clock.
    pub generated_at: String,
}

/// Text-specific fingerprint details. Sprint 5 E1 ships
/// `original_byte_len` + `nfc_char_count` for diagnostic display of the
/// pre-normalization input; E3 adds `minhash` + `simhash` for near-
/// duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFingerprint {
    /// Length of the **input** bytes before normalization. Useful for
    /// distinguishing whitespace-collapsed-equivalent documents in
    /// inclusion-proof evidence display.
    pub original_byte_len: u64,
    /// Number of Unicode scalar values (chars) in the post-NFC string,
    /// before lowercase + whitespace collapse. Diagnostic only; the hash
    /// input length is `FingerprintBundle.byte_len`.
    pub nfc_char_count: u64,
}

// ============================================================================
// FingerprintOpts
// ============================================================================

/// Caller-supplied options for fingerprint generation.
///
/// `source_date_epoch` is REQUIRED and never sourced from the system clock
/// — preserves byte-determinism across the 4-target CI matrix per the same
/// `--source-date-epoch` discipline established in Sprint 3 E3 for the
/// Parquet manifest writer (Reproducible Builds convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FingerprintOpts {
    /// Unix epoch seconds stamped into `FingerprintBundle.generated_at`.
    pub source_date_epoch: i64,
}

// ============================================================================
// Errors
// ============================================================================

#[derive(thiserror::Error, Debug)]
pub enum AttestrumFingerprintError {
    /// Input bytes were not valid UTF-8 (text path only). Image / bytes
    /// paths skip UTF-8 validation.
    #[error("invalid UTF-8 in text input: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// `source_date_epoch` was outside the range jiff can encode as a valid
    /// `Timestamp` (well-formed Unix seconds in the supported range —
    /// roughly year 1 to year 9999).
    #[error("invalid source_date_epoch {0}: not a representable Unix timestamp")]
    InvalidTimestamp(i64),

    /// Caller passed a [`Modality`] this crate does not yet implement in
    /// Sprint 5 (Audio / Video / Pdf). Sprint 5 supports Text + Image +
    /// Other-as-bytes; the unimplemented variants surface here from any
    /// future dispatch entry-point (E2's `fingerprint_bytes` will accept
    /// Other; a hypothetical `fingerprint_any(Modality, &[u8])` would
    /// surface this for the rest).
    #[error("modality {0:?} not yet implemented in attestrum-fingerprint v0.1")]
    ModalityNotImplemented(Modality),
}

// ============================================================================
// Public entry points
// ============================================================================

/// Fingerprint text content using the **PROTECTED** normalization pipeline.
///
/// Pipeline:
///
/// 1. UTF-8 validate via `std::str::from_utf8` — non-UTF-8 inputs surface
///    as [`AttestrumFingerprintError::InvalidUtf8`].
/// 2. NFC normalize via `unicode-normalization`'s `nfc()` iterator
///    (Unicode Standard Annex #15).
/// 3. `str::to_lowercase` — Unicode-aware case folding.
/// 4. `split_whitespace().collect::<Vec<_>>().join(" ")` — any run of
///    Unicode `White_Space`-property characters becomes a single ASCII
///    `0x20`; leading / trailing whitespace implicitly stripped.
/// 5. BLAKE3 + SHA-256 over the normalized UTF-8 bytes.
///
/// Returns a [`FingerprintBundle`] with `modality = Modality::Text`,
/// `text = Some(TextFingerprint { … })`, and `generated_at` derived from
/// the caller's `source_date_epoch`.
///
/// # PROTECTED
///
/// Changing the pipeline (steps 2-4 specifically) invalidates every
/// inclusion proof emitted to date. See crate-level docs.
pub fn fingerprint_text(
    bytes: &[u8],
    opts: &FingerprintOpts,
) -> Result<FingerprintBundle, AttestrumFingerprintError> {
    let original_byte_len = bytes.len() as u64;
    let input = std::str::from_utf8(bytes)?;

    // Compute NFC char count for diagnostic display BEFORE lowercase
    // collapse — captures the "Unicode shape" of the input separate from
    // the hash-input length.
    let nfc_char_count = input.nfc().count() as u64;

    let normalized = normalize_text(input);
    let normalized_bytes = normalized.as_bytes();

    let blake3 = blake3::hash(normalized_bytes);
    let sha256 = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(normalized_bytes);
        h.finalize()
    };

    let generated_at = jiff::Timestamp::from_second(opts.source_date_epoch)
        .map_err(|_| AttestrumFingerprintError::InvalidTimestamp(opts.source_date_epoch))?
        .to_string();

    Ok(FingerprintBundle {
        schema: FINGERPRINT_SCHEMA.to_string(),
        modality: Modality::Text,
        blake3: attestrum_core::hex::encode(blake3.as_bytes()),
        sha256: attestrum_core::hex::encode(&sha256),
        byte_len: normalized_bytes.len() as u64,
        text: Some(TextFingerprint {
            original_byte_len,
            nfc_char_count,
        }),
        generated_at,
    })
}

// ============================================================================
// Private helpers
// ============================================================================

/// PROTECTED text normalization pipeline (CLAUDE.md §4).
///
/// `NFC → str::to_lowercase → split_whitespace + " " join`. Implicit
/// leading / trailing whitespace strip via `split_whitespace`'s skip-empty
/// semantics.
fn normalize_text(input: &str) -> String {
    let nfc: String = input.nfc().collect();
    let lower = nfc.to_lowercase();
    lower.split_whitespace().collect::<Vec<&str>>().join(" ")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Consistent across the test module so any cross-test bundle comparison
    /// only varies on the input bytes. Mirrors the existing
    /// cosign_interop.rs convention (`1_748_109_600`).
    const TEST_EPOCH: i64 = 1_748_109_600;

    fn opts() -> FingerprintOpts {
        FingerprintOpts {
            source_date_epoch: TEST_EPOCH,
        }
    }

    // ----- normalize_text unit tests -----

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

    // ----- fingerprint_text integration tests -----

    #[test]
    fn fingerprint_text_basic_ascii() {
        let bundle = fingerprint_text(b"hello world", &opts()).unwrap();
        assert_eq!(bundle.modality, Modality::Text);
        assert_eq!(bundle.schema, FINGERPRINT_SCHEMA);
        assert_eq!(bundle.byte_len, "hello world".len() as u64);
        let text = bundle.text.as_ref().expect("text branch must be populated");
        assert_eq!(text.original_byte_len, "hello world".len() as u64);
        assert_eq!(text.nfc_char_count, 11);
        // Both digests are hex-64 lowercase.
        assert_eq!(bundle.blake3.len(), 64);
        assert_eq!(bundle.sha256.len(), 64);
        assert!(bundle.blake3.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(bundle.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fingerprint_text_nfc_equivalent_inputs_produce_identical_digests() {
        // The PROTECTED guarantee: byte-different but Unicode-equivalent
        // inputs MUST collapse to the same fingerprint. This is the
        // load-bearing invariant for inclusion proofs across publishers
        // that use different Unicode normalization conventions.
        let precomposed = fingerprint_text("café".as_bytes(), &opts()).unwrap();
        let decomposed = fingerprint_text("cafe\u{0301}".as_bytes(), &opts()).unwrap();
        assert_eq!(precomposed.blake3, decomposed.blake3);
        assert_eq!(precomposed.sha256, decomposed.sha256);
        assert_eq!(precomposed.byte_len, decomposed.byte_len);
        // The pre-normalization lengths DIFFER (the whole point of NFC) —
        // confirms TextFingerprint's diagnostic display would surface the
        // difference even though the hashes match.
        assert_ne!(
            precomposed.text.as_ref().unwrap().original_byte_len,
            decomposed.text.as_ref().unwrap().original_byte_len
        );
    }

    #[test]
    fn fingerprint_text_whitespace_variations_collapse_to_same_digest() {
        // The PROTECTED whitespace-collapse step means these three inputs
        // all hash to the same value.
        let a = fingerprint_text(b"hello world", &opts()).unwrap();
        let b = fingerprint_text(b"  hello   world  ", &opts()).unwrap();
        let c = fingerprint_text(b"hello\t\nworld", &opts()).unwrap();
        assert_eq!(a.blake3, b.blake3);
        assert_eq!(a.blake3, c.blake3);
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(a.sha256, c.sha256);
    }

    #[test]
    fn fingerprint_text_case_variations_collapse_to_same_digest() {
        let lower = fingerprint_text(b"hello world", &opts()).unwrap();
        let upper = fingerprint_text(b"HELLO WORLD", &opts()).unwrap();
        let mixed = fingerprint_text(b"HeLlO wOrLd", &opts()).unwrap();
        assert_eq!(lower.blake3, upper.blake3);
        assert_eq!(lower.blake3, mixed.blake3);
    }

    #[test]
    fn fingerprint_text_distinct_content_produces_distinct_digests() {
        let a = fingerprint_text(b"hello world", &opts()).unwrap();
        let b = fingerprint_text(b"goodbye world", &opts()).unwrap();
        assert_ne!(a.blake3, b.blake3);
        assert_ne!(a.sha256, b.sha256);
    }

    #[test]
    fn fingerprint_text_rejects_invalid_utf8() {
        // High bit set with no valid continuation — invalid UTF-8.
        let bad: [u8; 3] = [0xFF, 0xFE, 0xFD];
        let err = fingerprint_text(&bad, &opts()).unwrap_err();
        assert!(matches!(err, AttestrumFingerprintError::InvalidUtf8(_)));
    }

    #[test]
    fn fingerprint_text_rejects_unrepresentable_epoch() {
        // i64::MAX is well outside jiff's supported range (~year 9999).
        let bad_opts = FingerprintOpts {
            source_date_epoch: i64::MAX,
        };
        let err = fingerprint_text(b"hello", &bad_opts).unwrap_err();
        match err {
            AttestrumFingerprintError::InvalidTimestamp(t) => assert_eq!(t, i64::MAX),
            other => panic!("expected InvalidTimestamp, got {other:?}"),
        }
    }

    #[test]
    fn fingerprint_text_generated_at_derives_deterministically_from_epoch() {
        let a = fingerprint_text(b"x", &opts()).unwrap();
        let b = fingerprint_text(b"x", &opts()).unwrap();
        assert_eq!(a.generated_at, b.generated_at);
        // RFC 3339 form: "2025-05-24T18:00:00Z" for epoch 1_748_109_600.
        assert!(a.generated_at.starts_with("2025-05-24T18:00:00"));
        assert!(a.generated_at.ends_with("Z"));
    }

    #[test]
    fn fingerprint_text_empty_input_is_valid() {
        let bundle = fingerprint_text(b"", &opts()).unwrap();
        assert_eq!(bundle.byte_len, 0);
        assert_eq!(bundle.text.as_ref().unwrap().original_byte_len, 0);
        assert_eq!(bundle.text.as_ref().unwrap().nfc_char_count, 0);
        // BLAKE3("") is a known constant.
        assert_eq!(
            bundle.blake3,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    // ----- serde shape tests -----

    #[test]
    fn fingerprint_bundle_round_trips_via_serde_json() {
        let bundle = fingerprint_text(b"hello world", &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        let back: FingerprintBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle, back);
    }

    #[test]
    fn fingerprint_bundle_serializes_camel_case_keys() {
        let bundle = fingerprint_text(b"hello", &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        // Top-level fields:
        assert!(json.contains("\"schema\""));
        assert!(json.contains("\"modality\""));
        assert!(json.contains("\"blake3\""));
        assert!(json.contains("\"sha256\""));
        assert!(json.contains("\"byteLen\""));
        assert!(json.contains("\"generatedAt\""));
        // TextFingerprint nested fields:
        assert!(json.contains("\"originalByteLen\""));
        assert!(json.contains("\"nfcCharCount\""));
        // Modality enum: PascalCase per attestrum-core::Modality's default
        // serde shape (no rename_all on that enum, by deliberate workspace
        // convention — see attestrum_core::tests).
        assert!(json.contains("\"Text\""));
    }

    #[test]
    fn fingerprint_bundle_omits_text_field_when_none() {
        // Hand-construct a bundle with text=None to exercise the skip-if-None.
        let bundle = FingerprintBundle {
            schema: FINGERPRINT_SCHEMA.to_string(),
            modality: Modality::Other,
            blake3: "0".repeat(64),
            sha256: "0".repeat(64),
            byte_len: 0,
            text: None,
            generated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(!json.contains("\"text\""));
    }

    // ----- Modality re-export sanity -----

    #[test]
    fn modality_re_export_is_attestrum_core_modality() {
        // Verifies the re-export resolves to the same type at the type
        // system level — `Modality` in this crate IS `attestrum_core::Modality`.
        fn assert_same_type<T>(_a: T, _b: T) {}
        assert_same_type(Modality::Text, attestrum_core::Modality::Text);
    }
}
