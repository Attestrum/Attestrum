//! `attestrum-fingerprint` — deterministic per-document fingerprints for the
//! Sprint 5 [`attestrum prove`] workflow. Pure functions; no I/O beyond the
//! caller-supplied input bytes; no system-clock reads (cross-target byte
//! determinism per CLAUDE.md §7).
//!
//! See `docs/diagrams/sprint-5/fingerprint-pipeline.md` for the pipeline
//! diagram (the single diagram covering all of S5-D1; per-E-commit updates
//! bump its `last_verified` SHA rather than spawning per-commit diagrams).
//!
//! Sprint 5 E1 ships the text branch:
//! [`fingerprint_text`] applies the **PROTECTED** normalization pipeline
//! (NFC → `str::to_lowercase` → whitespace collapse) and produces a
//! [`FingerprintBundle`] carrying both BLAKE3 (Attestrum-native, Merkle leaf
//! input) and SHA-256 (Sigstore / in-toto interop) digests of the normalized
//! bytes.
//!
//! Sprint 5 E2 adds the image branch: [`fingerprint_image`]
//! decodes via the `image` crate, computes a DCT-based 64-bit pHash via
//! `image_hasher` (`HasherConfig::new().hash_size(8, 8).preproc_dct()`), a
//! 64-bit blockhash via the `blockhash` crate (blockhash.io spec), and the
//! BLAKE3 + SHA-256 over the RAW input bytes — distinct from text's
//! over-normalized-bytes semantics because image exact-match means "same
//! encoded file" not "same decoded pixels".
//!
//! Sprint 5 E3 adds two near-duplicate-detection hashes to the text
//! branch: MinHash (128 BLAKE3-keyed permutations over 5-gram word
//! shingles) and SimHash (64-bit, BLAKE3-keyed, uniform-weighted). Both
//! run over the already-PROTECTED-normalized text produced by
//! [`normalize_text`] and are populated unconditionally by
//! [`fingerprint_text`]; downstream `attestrum-prove` (Sprint 5 E9)
//! consumes them via `MatchEvidence::MinHash`. Implementation lives under
//! `src/text/{mod,minhash,simhash}.rs` (`pub(crate)`; no external dep —
//! hand-rolled per PATH-A-BRIEF Part 2.1 line 522).
//!
//! Sprint 5 E4 adds ISO 24138:2024 ISCC composition via the
//! official `iscc-lib 0.4` Rust-core crate (PATH-A-BRIEF Part 2.1 line
//! 532): [`fingerprint_text`] + [`fingerprint_image`] populate the new
//! [`FingerprintBundle::iscc`] field (an [`IsccComposition`] of four
//! strings — content code, data code, instance code, and the composite).
//! Text content-code uses the RAW input text (per the ISCC spec —
//! iscc-lib applies its own normalization internally), DISTINCT from the
//! PROTECTED-normalized text consumed by BLAKE3 + SHA-256 + MinHash +
//! SimHash. Image content-code uses a 32×32 grayscale Lanczos3 resize of
//! the decoded image (the canonical ISCC pre-processing). Downstream
//! `attestrum-prove` (Sprint 5 E9) consumes the composite via
//! `MatchEvidence::Iscc { composite_distance }`.
//!
//! Sprint 5 E5 (this commit) closes S5-D1 by freezing the public API
//! surface and asserting cross-target byte determinism. Three new test
//! artifacts gate the contract: a `tests/api_surface.rs` golden-file
//! snapshot of every `pub` item in `src/lib.rs` (mirroring the proven
//! `attestrum-attest` precedent — accidental `pub` additions / renames /
//! signature shifts break the diff); a `tests/schema_derive.rs`
//! schemars-derived JSON Schema published at
//! `attestrum.com/fingerprint/v0.1.schema.json` and pinned via
//! `docs/schemas/fingerprint-v0.1.schema.json`; and a `tests/determinism.rs`
//! byte-identity check against committed PNG fixtures under
//! `tests/fixtures/` (the `cargo test --workspace` invocation in the
//! existing `determinism.yml` 4-target matrix catches inter-target drift
//! on the same golden). [`FingerprintBundle`] / [`TextFingerprint`] /
//! [`ImageFingerprint`] / [`IsccComposition`] now derive
//! `schemars::JsonSchema`; the [`Modality`] re-export inherits the same
//! derive from `attestrum_core` (paired commit). Diagram source-of-truth
//! flipped from `diagram` to `code` for `docs/diagrams/sprint-5/
//! fingerprint-pipeline.md` — the Rust types are now authoritative; the
//! diagram becomes the derived view.
//!
//! # PROTECTED
//!
//! Per CLAUDE.md §4: once any inclusion proof is emitted citing the
//! `attestrum.com/fingerprint/v0.1` schema URI, all of the following are
//! immutable:
//!
//! - The text-normalization pipeline ([`normalize_text`]).
//! - The MinHash / SimHash algorithm parameters (`src/text/`).
//! - The ISCC composition recipe (this commit): `iscc-lib 0.4` version
//!   pin, raw-text input for `gen_text_code_v0`, 32×32 grayscale
//!   Lanczos3 resize for `gen_image_code_v0`, 64-bit per unit,
//!   `gen_iscc_code_v0` over `[content_code, data_code, instance_code]`
//!   with `wide = false`, [`IsccComposition`] serde shape.
//!
//! Any such change requires both a `Protected-system-change:` commit-
//! message footer AND a schema URI bump from `…/fingerprint/v0.1` →
//! `…/fingerprint/v0.2` with a migration packet. The MinHash + SimHash
//! parameter lock landed in the Sprint 5 E3 commit; the ISCC composition
//! parameter lock landed in this commit (both founder-approved
//! 2026-05-25).
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

// Sprint 5 E3: PROTECTED MinHash + SimHash implementations.
// Private to the crate — the public surface change is the two new fields
// on `TextFingerprint`; the compute helpers themselves are implementation
// detail consumed only by `fingerprint_text` below.
mod text;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// Image-specific details. Present iff `modality == Modality::Image`.
    /// Sprint 5 E2 addition; non-breaking for E1-emitted text bundles
    /// because `None` is omitted via `skip_serializing_if`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageFingerprint>,
    /// ISCC composition per ISO 24138:2024. Present for text and image
    /// modalities (Sprint 5 E4); will be `None` for the future Other-as-
    /// bytes path. Non-breaking serde addition (`skip_serializing_if`):
    /// E1 / E2 / E3-emitted bundles remain byte-identical when the field
    /// is `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iscc: Option<IsccComposition>,
    /// Deterministic timestamp (RFC 3339). Set from
    /// `FingerprintOpts.source_date_epoch` — never the system clock.
    pub generated_at: String,
}

/// Text-specific fingerprint details.
///
/// Diagnostic fields (`original_byte_len`, `nfc_char_count`) describe the
/// pre-normalization input. The `minhash` Vec (128 BLAKE3-keyed 5-gram-
/// shingle min-hashes) and the `simhash` u64 (64-bit, BLAKE3-keyed,
/// uniform-weighted) are near-duplicate-detection hashes computed over
/// the PROTECTED-normalized text. Both algorithm parameter sets are
/// locked as of Sprint 5 E3 — see crate-level PROTECTED block.
///
/// Caller-side downstream code computes:
///
/// - **Jaccard similarity** from `minhash` as
///   `a.minhash.iter().zip(b.minhash.iter()).filter(|(x, y)| x == y).count() as f64 / 128.0`.
/// - **Hamming distance** from `simhash` as `(a.simhash ^ b.simhash).count_ones()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// 128 BLAKE3-keyed MinHash values over 5-gram word shingles of the
    /// PROTECTED-normalized text. Length is always exactly 128. Empty
    /// input yields `vec![u64::MAX; 128]`. PROTECTED — see crate-level
    /// PROTECTED block for the locked algorithm parameters.
    pub minhash: Vec<u64>,
    /// 64-bit BLAKE3-keyed SimHash with uniform weights over 5-gram word
    /// shingles of the PROTECTED-normalized text. Empty input yields `0`.
    /// PROTECTED — see crate-level PROTECTED block for the locked
    /// algorithm parameters.
    pub simhash: u64,
}

/// Image-specific fingerprint details. Sprint 5 E2 ships two 64-bit
/// perceptual hashes (different attack-surface profiles):
///
/// - [`Self::phash`] — DCT-based 64-bit perceptual hash via
///   `image_hasher::HasherConfig::new().hash_size(8, 8).preproc_dct()`.
///   Sensitive to subtle tonal changes; robust to small geometric
///   distortions and re-encoding. **Approximately deterministic
///   cross-target, NOT byte-identical**: `image_hasher 3.1.1` uses
///   `f32` internally for the DCT, and f32 math is not guaranteed
///   byte-identical across rustc + LLVM + target libc combinations.
///   Empirically the cross-target Hamming drift is ≤ 1-2 bits per 64
///   for inputs whose DCT coefficients sit away from the median
///   threshold, but high-frequency content (sharp checkerboards) can
///   shift up to ~8 bits per 64. Downstream `attestrum-prove` consumers
///   handle this natively via [`MatchEvidence::Perceptual`]'s
///   `threshold` field — the threshold IS the cross-target tolerance.
///   Exact-byte cross-target identity is provided by [`Self::blockhash`]
///   and the parent [`FingerprintBundle`]'s blake3/sha256 fields.
///   See `tests/determinism.rs` `normalize_phash_for_cross_target` for
///   the determinism-test-side handling. Documented as a known
///   limitation at Sprint 5 S5-D1 E5 fix-forward (2026-05-26).
/// - [`Self::blockhash`] — 64-bit blockhash.io spec hash via
///   `blockhash::blockhash64`. Block-mean approach; robust to colour /
///   tonal manipulation; more sensitive to geometric distortions than
///   pHash. Integer-only — **fully cross-target byte-identical**.
///
/// Inclusion-proof [`MatchEvidence::Perceptual`] in `attestrum-attest`
/// carries `hamming_distance` + `threshold` over one of these hashes;
/// non-inclusion proofs cite both for completeness. The decision of
/// which hash to use for a given match is the consumer's
/// (`attestrum-prove` at E9) — both are emitted here unconditionally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageFingerprint {
    /// DCT-based 64-bit pHash, hex-encoded lowercase (16 chars).
    pub phash: String,
    /// 64-bit blockhash.io spec hash, hex-encoded lowercase (16 chars).
    pub blockhash: String,
    /// Original image width in pixels (pre-resize). Diagnostic only —
    /// not load-bearing for hash comparison.
    pub width: u32,
    /// Original image height in pixels (pre-resize). Diagnostic only.
    pub height: u32,
}

/// ISO 24138:2024 ISCC composition produced by `iscc-lib 0.4`.
///
/// Four base32-encoded ISCC unit code strings (each starting with the
/// `"ISCC:"` prefix). Sprint 5 E4 ships content + data + instance unit
/// codes and the composite `gen_iscc_code_v0` over them; meta-code is
/// omitted because we have no metadata input.
///
/// Caller-side downstream code computes **composite distance** via
/// `iscc_decompose` + Hamming over the binary digest representation of
/// two composites — that lives in `attestrum-prove` (Sprint 5 E9), not
/// here. The fingerprint crate's job is to emit the codes; the distance
/// computation is the consumer's.
///
/// PROTECTED algorithm parameters (see crate-level docs):
///
/// - Text content-code: `gen_text_code_v0(raw_input_text, 64)` — RAW
///   input text per ISCC spec, NOT the PROTECTED-normalized text.
/// - Image content-code: `gen_image_code_v0(&pixels[..1024], 64)` over
///   a 32×32 grayscale `image::imageops::FilterType::Lanczos3` resize
///   of the decoded image.
/// - Data code: `gen_data_code_v0(raw_bytes, 64)`.
/// - Instance code: `gen_instance_code_v0(raw_bytes, 64)`.
/// - Composite: `gen_iscc_code_v0(&[content, data, instance], false)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsccComposition {
    /// Content-code (Text-Code-v0 for text inputs, Image-Code-v0 for
    /// image inputs). Base32-encoded; starts with `"ISCC:"` prefix.
    pub content_code: String,
    /// Data-Code-v0 (similarity hash over the raw input bytes).
    pub data_code: String,
    /// Instance-Code-v0 (cryptographic hash over the raw input bytes).
    pub instance_code: String,
    /// Composite ISCC-CODE from `gen_iscc_code_v0(&[content, data,
    /// instance], wide=false)` — the canonical ISO 24138 ISCC string
    /// downstream `MatchEvidence::Iscc` evidence is measured against.
    pub composite: String,
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
    /// future dispatch entry-point (a hypothetical
    /// `fingerprint_any(Modality, &[u8])` would surface this for Audio /
    /// Video / Pdf).
    #[error("modality {0:?} not yet implemented in attestrum-fingerprint v0.1")]
    ModalityNotImplemented(Modality),

    /// Input bytes could not be decoded as any supported image format
    /// (image-path only). The wrapped string is the underlying `image`
    /// crate's error message; we wrap rather than carry the typed error
    /// so this crate's public-error type doesn't drag `image::ImageError`
    /// into downstream callers' type surface.
    #[error("image decode failed: {0}")]
    ImageDecode(String),

    /// `iscc-lib` returned an error during ISCC unit-code generation or
    /// composition (Sprint 5 E4+). The wrapped string is iscc-lib's
    /// `IsccError` Display output; we wrap rather than re-export the
    /// typed error so this crate's public surface doesn't drag
    /// `iscc_lib::IsccError` into downstream callers' type surface.
    /// Variant name + message format pinned by PATH-A-BRIEF Part 2.1
    /// line 496.
    #[error("iscc backend failed: {0}")]
    IsccBackend(String),
}

impl From<iscc_lib::IsccError> for AttestrumFingerprintError {
    fn from(err: iscc_lib::IsccError) -> Self {
        Self::IsccBackend(err.to_string())
    }
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

    // Sprint 5 E3: PROTECTED MinHash 128 + SimHash 64 over the
    // already-PROTECTED-normalized text. Populated unconditionally — no
    // opts flag for skipping. See `text::minhash` / `text::simhash` for
    // the locked algorithm parameters.
    let minhash = text::minhash::compute(&normalized);
    let simhash = text::simhash::compute(&normalized);

    // Sprint 5 E4: PROTECTED ISCC composition over the RAW input text
    // (per ISCC spec — iscc-lib applies its own normalization internally).
    // See `compose_iscc` for the locked algorithm parameters.
    let iscc = compose_iscc(IsccContentInput::Text(input), bytes)?;

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
            minhash,
            simhash,
        }),
        image: None,
        iscc: Some(iscc),
        generated_at,
    })
}

/// Fingerprint image content. Decodes the encoded image bytes (PNG /
/// JPEG / WebP / BMP / GIF / TIFF supported via the `image` crate's
/// feature set), computes two 64-bit perceptual hashes (pHash + blockhash
/// — see [`ImageFingerprint`] for the algorithm choice rationale), and
/// derives BLAKE3 + SHA-256 over the **raw input bytes** (the encoded
/// file).
///
/// Returns a [`FingerprintBundle`] with `modality = Modality::Image`,
/// `image = Some(ImageFingerprint { … })`, `text = None`, and
/// `generated_at` derived from the caller's `source_date_epoch`.
///
/// # Exact-match semantics
///
/// `blake3` / `sha256` are over the **raw input bytes** (the encoded
/// file's bytes), NOT the decoded pixel data. Inclusion-proof
/// [`MatchEvidence::ExactBlake3`] / [`MatchEvidence::ExactSha256`] paths
/// therefore mean "the corpus contains this same encoded file"; a
/// re-encoded copy (lossy JPEG re-save, format conversion, etc.) will
/// have a different exact-match digest but will likely retain matching
/// perceptual hashes — that's the [`MatchEvidence::Perceptual`] path's
/// job. This is a deliberate divergence from the text path, where
/// exact-match means "same normalized content"; for images the canonical
/// form is the encoded bytes because lossy re-encoding is the dominant
/// variance source publishers actually encounter.
///
/// [`MatchEvidence::ExactBlake3`]: https://docs.rs/attestrum-attest/
/// [`MatchEvidence::ExactSha256`]: https://docs.rs/attestrum-attest/
/// [`MatchEvidence::Perceptual`]: https://docs.rs/attestrum-attest/
pub fn fingerprint_image(
    bytes: &[u8],
    opts: &FingerprintOpts,
) -> Result<FingerprintBundle, AttestrumFingerprintError> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AttestrumFingerprintError::ImageDecode(e.to_string()))?;
    let width = img.width();
    let height = img.height();

    // DCT-based pHash: 8x8 = 64 bits, with preproc_dct to apply discrete
    // cosine transform to a larger resize-target before downsampling to
    // the 8x8 hash size (the "real" pHash recipe per Marr / pHash.org).
    // image_hasher's default HashAlg::Gradient + preproc_dct() composes
    // into a DCT-based perceptual hash; without preproc_dct it's dHash.
    let phash_hasher = image_hasher::HasherConfig::new()
        .hash_size(8, 8)
        .preproc_dct()
        .to_hasher();
    let phash = phash_hasher.hash_image(&img);

    // 64-bit blockhash.io spec hash via the separate `blockhash` crate.
    // Distinct algorithm class from image_hasher's HashAlg::Blockhash
    // variant (which is image_hasher's reimplementation); the dedicated
    // crate is the canonical implementation of the blockhash.io spec.
    let bh = blockhash::blockhash64(&img);

    // BLAKE3 + SHA-256 over RAW input bytes (see docstring exact-match
    // semantics note).
    let blake3 = blake3::hash(bytes);
    let sha256 = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(bytes);
        h.finalize()
    };

    // Sprint 5 E4: PROTECTED ISCC composition over the 32×32 grayscale
    // Lanczos3 resize of the decoded image. See `compose_iscc` +
    // `iscc_image_pixels` for the locked pipeline.
    let iscc_pixels = iscc_image_pixels(&img);
    let iscc = compose_iscc(IsccContentInput::Image(&iscc_pixels), bytes)?;

    let generated_at = jiff::Timestamp::from_second(opts.source_date_epoch)
        .map_err(|_| AttestrumFingerprintError::InvalidTimestamp(opts.source_date_epoch))?
        .to_string();

    Ok(FingerprintBundle {
        schema: FINGERPRINT_SCHEMA.to_string(),
        modality: Modality::Image,
        blake3: attestrum_core::hex::encode(blake3.as_bytes()),
        sha256: attestrum_core::hex::encode(&sha256),
        byte_len: bytes.len() as u64,
        text: None,
        image: Some(ImageFingerprint {
            phash: attestrum_core::hex::encode(phash.as_bytes()),
            // blockhash::Blockhash64's Display impl is lowercase hex.
            blockhash: bh.to_string(),
            width,
            height,
        }),
        iscc: Some(iscc),
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

/// PROTECTED ISCC image pre-processing pipeline (CLAUDE.md §4, Sprint 5
/// E4 lock).
///
/// Decode → `to_luma8` → `resize_exact(32, 32, Lanczos3)` → `into_raw`.
/// The fixed `[u8; 1024]` return type enforces the
/// `iscc_lib::gen_image_code_v0` input contract (1024 = 32 × 32 grayscale
/// pixels) at compile time. The filter choice (Lanczos3) matches the
/// canonical ISCC spec; the `image` crate's Lanczos3 implementation is
/// integer-only + deterministic across our 4 CI targets.
fn iscc_image_pixels(img: &image::DynamicImage) -> [u8; 1024] {
    let resized = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
    let gray = resized.to_luma8();
    let pixels: Vec<u8> = gray.into_raw();
    pixels
        .try_into()
        .expect("32x32 Luma8 yields exactly 1024 pixels by construction")
}

/// Either-or dispatch input for the [`compose_iscc`] helper. Text path
/// hands a `&str` (iscc-lib applies its own normalization internally);
/// image path hands the pre-processed 1024-pixel grayscale slice from
/// [`iscc_image_pixels`].
enum IsccContentInput<'a> {
    Text(&'a str),
    Image(&'a [u8; 1024]),
}

/// PROTECTED ISCC composition pipeline (CLAUDE.md §4, Sprint 5 E4 lock).
///
/// Produces an [`IsccComposition`] by calling `iscc-lib 0.4`:
///
/// 1. Content code: `gen_text_code_v0(raw_text, 64)` OR
///    `gen_image_code_v0(&pixels, 64)` depending on `content_input`.
/// 2. Data code: `gen_data_code_v0(raw_bytes, 64)`.
/// 3. Instance code: `gen_instance_code_v0(raw_bytes, 64)`.
/// 4. Composite: `gen_iscc_code_v0(&[content, data, instance], false)`.
///
/// `iscc_lib::IsccError` is auto-converted to
/// [`AttestrumFingerprintError::IsccBackend`] via the `From` impl above.
fn compose_iscc(
    content_input: IsccContentInput<'_>,
    raw_bytes: &[u8],
) -> Result<IsccComposition, AttestrumFingerprintError> {
    let content_code = match content_input {
        IsccContentInput::Text(text) => iscc_lib::gen_text_code_v0(text, 64)?.iscc,
        IsccContentInput::Image(pixels) => iscc_lib::gen_image_code_v0(pixels, 64)?.iscc,
    };
    let data_code = iscc_lib::gen_data_code_v0(raw_bytes, 64)?.iscc;
    let instance_code = iscc_lib::gen_instance_code_v0(raw_bytes, 64)?.iscc;
    let composite =
        iscc_lib::gen_iscc_code_v0(&[&content_code, &data_code, &instance_code], false)?.iscc;
    Ok(IsccComposition {
        content_code,
        data_code,
        instance_code,
        composite,
    })
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
        // E3: MinHash 128 + SimHash 64 populated unconditionally.
        assert_eq!(
            text.minhash.len(),
            128,
            "TextFingerprint.minhash must be exactly 128 entries"
        );
        // E4: ISCC composition populated.
        let iscc = bundle.iscc.as_ref().expect("E4 iscc branch must populate");
        assert!(
            iscc.composite.starts_with("ISCC:"),
            "iscc.composite must start with the ISCC: prefix"
        );
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
    fn fingerprint_bundle_omits_text_image_and_iscc_fields_when_none() {
        // Hand-construct a bundle with text=None AND image=None AND
        // iscc=None to exercise all three skip-if-None branches (the
        // future Other-as-bytes path). E4 added the iscc field; this
        // test now covers all three optional branches.
        let bundle = FingerprintBundle {
            schema: FINGERPRINT_SCHEMA.to_string(),
            modality: Modality::Other,
            blake3: "0".repeat(64),
            sha256: "0".repeat(64),
            byte_len: 0,
            text: None,
            image: None,
            iscc: None,
            generated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(!json.contains("\"text\""));
        assert!(!json.contains("\"image\""));
        assert!(!json.contains("\"iscc\""));
    }

    // ----- Modality re-export sanity -----

    #[test]
    fn modality_re_export_is_attestrum_core_modality() {
        // Verifies the re-export resolves to the same type at the type
        // system level — `Modality` in this crate IS `attestrum_core::Modality`.
        fn assert_same_type<T>(_a: T, _b: T) {}
        assert_same_type(Modality::Text, attestrum_core::Modality::Text);
    }

    // ========================================================================
    // E3: text near-duplicate hashes (MinHash + SimHash) — end-to-end via
    // fingerprint_text. Submodule-level unit tests live in
    // `src/text/minhash.rs` + `src/text/simhash.rs`; these tests verify the
    // wire-up + cross-cutting invariants (NFC + whitespace equivalence,
    // serde shape).
    // ========================================================================

    #[test]
    fn fingerprint_text_populates_minhash_and_simhash_unconditionally() {
        // Non-trivial input (>5 tokens) exercises the full shingle path.
        let bundle = fingerprint_text(
            b"the quick brown fox jumps over the lazy dog in the meadow",
            &opts(),
        )
        .unwrap();
        let text = bundle.text.as_ref().expect("text branch must be populated");
        assert_eq!(
            text.minhash.len(),
            128,
            "minhash must be exactly 128 entries"
        );
        // Cryptographic-hash output is overwhelmingly unlikely to be 0 for a
        // multi-shingle input — assert_ne is a meaningful sanity check that
        // the SimHash is actually populated (not left at its accumulator
        // default).
        assert_ne!(
            text.simhash, 0,
            "simhash must populate to a non-zero value for non-trivial input"
        );
    }

    #[test]
    fn fingerprint_text_nfc_equivalent_inputs_produce_identical_minhash_simhash() {
        // Load-bearing: confirms MinHash + SimHash ride on top of the
        // PROTECTED NFC normalization correctly. Byte-different but
        // Unicode-equivalent inputs MUST collapse to identical near-
        // duplicate hashes (otherwise the inclusion-proof verification
        // surface fragments across NFC / NFD-emitting publishers).
        let precomposed = fingerprint_text("café au lait".as_bytes(), &opts()).unwrap();
        let decomposed = fingerprint_text("cafe\u{0301} au lait".as_bytes(), &opts()).unwrap();
        let p_text = precomposed.text.as_ref().unwrap();
        let d_text = decomposed.text.as_ref().unwrap();
        assert_eq!(p_text.minhash, d_text.minhash);
        assert_eq!(p_text.simhash, d_text.simhash);
    }

    #[test]
    fn fingerprint_text_whitespace_variations_produce_identical_minhash_simhash() {
        // Same logic as the NFC test, for whitespace-collapse equivalence.
        let a = fingerprint_text(b"the quick brown fox jumps over the lazy dog", &opts()).unwrap();
        let b = fingerprint_text(
            b"  the   quick\tbrown\nfox  jumps\tover  the   lazy   dog  ",
            &opts(),
        )
        .unwrap();
        let a_text = a.text.as_ref().unwrap();
        let b_text = b.text.as_ref().unwrap();
        assert_eq!(a_text.minhash, b_text.minhash);
        assert_eq!(a_text.simhash, b_text.simhash);
    }

    #[test]
    fn fingerprint_text_bundle_serializes_minhash_and_simhash_as_camelcase() {
        let bundle = fingerprint_text(b"hello world from the e3 commit", &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        // Field names: minhash + simhash are already lowercase camelCase
        // (no internal capitals) so serde's rename_all="camelCase" leaves
        // them as-is.
        assert!(
            json.contains("\"minhash\""),
            "serialized JSON must carry minhash key; got {json}"
        );
        assert!(
            json.contains("\"simhash\""),
            "serialized JSON must carry simhash key; got {json}"
        );
        // Round-trip preserves both fields' values exactly (Vec<u64> + u64
        // serde derives are lossless).
        let back: FingerprintBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(
            bundle.text.as_ref().unwrap().minhash,
            back.text.as_ref().unwrap().minhash
        );
        assert_eq!(
            bundle.text.as_ref().unwrap().simhash,
            back.text.as_ref().unwrap().simhash
        );
    }

    // ========================================================================
    // E4: ISCC composition (ISO 24138:2024 via iscc-lib 0.4) — end-to-end
    // through fingerprint_text + fingerprint_image. Tests cover shape (all
    // 4 ISCC strings present + prefixed "ISCC:"), determinism (same input
    // → same composition), distinctness (different inputs → different
    // composite), and the serde round-trip preserving every IsccComposition
    // field. PROTECTED at this commit per CLAUDE.md §4.
    // ========================================================================

    #[test]
    fn fingerprint_text_populates_iscc_composition() {
        let bundle = fingerprint_text(
            b"the quick brown fox jumps over the lazy dog in the meadow",
            &opts(),
        )
        .unwrap();
        let iscc = bundle.iscc.as_ref().expect("text path must populate iscc");
        // All 4 unit codes must be ISCC-prefixed non-empty strings.
        for (label, code) in [
            ("content_code", &iscc.content_code),
            ("data_code", &iscc.data_code),
            ("instance_code", &iscc.instance_code),
            ("composite", &iscc.composite),
        ] {
            assert!(
                code.starts_with("ISCC:"),
                "iscc.{label} = {code:?} must start with ISCC: prefix"
            );
            assert!(
                code.len() > "ISCC:".len(),
                "iscc.{label} must carry payload after the prefix"
            );
        }
    }

    #[test]
    fn fingerprint_text_iscc_is_deterministic() {
        let a = fingerprint_text(b"hello deterministic world", &opts()).unwrap();
        let b = fingerprint_text(b"hello deterministic world", &opts()).unwrap();
        assert_eq!(a.iscc, b.iscc, "same text input must yield identical iscc");
    }

    #[test]
    fn fingerprint_text_distinct_content_distinct_composite() {
        // Different inputs MUST produce different ISCC composite codes.
        // Cryptographic-distinctness sanity check (catches "compose
        // returns a constant" failure modes).
        let a = fingerprint_text(b"the quick brown fox", &opts()).unwrap();
        let b = fingerprint_text(b"a completely different document", &opts()).unwrap();
        let a_iscc = a.iscc.as_ref().unwrap();
        let b_iscc = b.iscc.as_ref().unwrap();
        assert_ne!(a_iscc.composite, b_iscc.composite);
        // Instance codes (cryptographic hash) MUST differ for different bytes.
        assert_ne!(a_iscc.instance_code, b_iscc.instance_code);
    }

    #[test]
    fn fingerprint_text_iscc_bundle_round_trips_via_serde_json() {
        let bundle = fingerprint_text(b"round trip me through serde", &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        let back: FingerprintBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.iscc, back.iscc);
        // Confirm camelCase JSON keys for the new IsccComposition fields.
        assert!(json.contains("\"iscc\""));
        assert!(json.contains("\"contentCode\""));
        assert!(json.contains("\"dataCode\""));
        assert!(json.contains("\"instanceCode\""));
        assert!(json.contains("\"composite\""));
    }

    // Image-branch ISCC tests live below (after the E2 fixture helpers
    // are declared) because they consume `checkerboard_png_bytes` /
    // `gradient_png_bytes`. The test module's item-order doesn't affect
    // visibility (Rust scopes by module, not by source order), but
    // keeping the test next to its fixtures aids navigation.

    // ========================================================================
    // E2: image fingerprint tests
    // ========================================================================
    //
    // Test images are generated programmatically via image::ImageBuffer +
    // re-encoded to in-memory PNG bytes via image::DynamicImage::write_to.
    // This keeps the test self-contained (no committed PNG fixtures) and
    // produces deterministic input bytes for repeatable assertions. The
    // committed-binary-fixture pattern lands at E5 alongside the cross-
    // target byte-determinism gate where the encoded-bytes round-trip
    // stability is the property under test.

    use image::{DynamicImage, ImageBuffer, ImageFormat, Luma};
    use std::io::Cursor;

    /// Build a 32x32 grayscale checkerboard with the given tile size.
    /// `tile_size = 4` yields a high-contrast 8x8-cell pattern; `tile_size = 8`
    /// yields a 4x4-cell pattern. Different tile sizes produce visibly
    /// different images with different perceptual hashes.
    fn checkerboard_png_bytes(tile_size: u32) -> Vec<u8> {
        let dim = 32u32;
        let buf: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(dim, dim, |x, y| {
            let cell_x = x / tile_size;
            let cell_y = y / tile_size;
            if (cell_x + cell_y) % 2 == 0 {
                Luma([255u8])
            } else {
                Luma([0u8])
            }
        });
        let img = DynamicImage::ImageLuma8(buf);
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("PNG encode of checkerboard test fixture");
        out
    }

    /// Build a 32x32 horizontal gradient (left=black, right=white). Used as
    /// a visually-distinct comparison image.
    fn gradient_png_bytes() -> Vec<u8> {
        let dim = 32u32;
        let buf: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(dim, dim, |x, _y| {
            // Map x in [0..32) to brightness [0..255].
            let v = (x * 255 / (dim - 1)).min(255) as u8;
            Luma([v])
        });
        let img = DynamicImage::ImageLuma8(buf);
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("PNG encode of gradient test fixture");
        out
    }

    /// Decode a hex-encoded perceptual hash back to `[u8; 8]` for Hamming-
    /// distance computation in tests. Both phash + blockhash are 64-bit
    /// (8 bytes) hex strings.
    fn decode_hex_8(hex: &str) -> [u8; 8] {
        assert_eq!(hex.len(), 16, "expected 16-char hex (64-bit hash)");
        let mut out = [0u8; 8];
        for i in 0..8 {
            let byte =
                u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hash hex must decode");
            out[i] = byte;
        }
        out
    }

    /// Hamming distance between two 64-bit hashes (bit count of XOR).
    fn hamming_distance_64(a: [u8; 8], b: [u8; 8]) -> u32 {
        (0..8).map(|i| (a[i] ^ b[i]).count_ones()).sum()
    }

    #[test]
    fn fingerprint_image_basic_shape() {
        let png = checkerboard_png_bytes(4);
        let bundle = fingerprint_image(&png, &opts()).unwrap();

        assert_eq!(bundle.modality, Modality::Image);
        assert_eq!(bundle.schema, FINGERPRINT_SCHEMA);
        assert_eq!(bundle.byte_len, png.len() as u64);
        assert!(
            bundle.text.is_none(),
            "image bundle must NOT carry text branch"
        );
        let img = bundle.image.as_ref().expect("image branch must populate");
        // 64-bit hashes encode to 16 hex chars each.
        assert_eq!(
            img.phash.len(),
            16,
            "phash should be 16 hex chars (64 bits)"
        );
        assert_eq!(
            img.blockhash.len(),
            16,
            "blockhash should be 16 hex chars (64 bits)"
        );
        assert!(img.phash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(img.blockhash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(img.width, 32);
        assert_eq!(img.height, 32);
        // Raw-bytes BLAKE3 + SHA-256 land as 64-char hex.
        assert_eq!(bundle.blake3.len(), 64);
        assert_eq!(bundle.sha256.len(), 64);
        // E4: ISCC composition populated for image branch.
        let iscc = bundle.iscc.as_ref().expect("E4 iscc branch must populate");
        assert!(
            iscc.composite.starts_with("ISCC:"),
            "iscc.composite must start with the ISCC: prefix"
        );
    }

    #[test]
    fn fingerprint_image_deterministic_for_same_input_bytes() {
        // The PROTECTED guarantee for the image path: same encoded bytes
        // produce byte-identical FingerprintBundles. Mirrors the text
        // path's same-input-same-output property.
        let png = checkerboard_png_bytes(4);
        let a = fingerprint_image(&png, &opts()).unwrap();
        let b = fingerprint_image(&png, &opts()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_image_distinct_images_distinct_exact_digests() {
        // Visually-distinct images MUST have distinct BLAKE3 / SHA-256
        // (because raw input bytes differ — the encoded PNGs are
        // different). Perceptual hashes will ALSO differ for these
        // sufficiently-distinct test images.
        let checker = fingerprint_image(&checkerboard_png_bytes(4), &opts()).unwrap();
        let gradient = fingerprint_image(&gradient_png_bytes(), &opts()).unwrap();
        assert_ne!(checker.blake3, gradient.blake3);
        assert_ne!(checker.sha256, gradient.sha256);
        let checker_img = checker.image.as_ref().unwrap();
        let gradient_img = gradient.image.as_ref().unwrap();
        assert_ne!(checker_img.phash, gradient_img.phash);
        assert_ne!(checker_img.blockhash, gradient_img.blockhash);
    }

    #[test]
    fn fingerprint_image_perceptual_hashes_differ_meaningfully_between_distinct_images() {
        // Robustness sanity: a checkerboard and a gradient should have
        // perceptual-hash Hamming distance well above zero. Thresholds
        // calibrated at S5-D1 E5 against observed values on macOS aarch64
        // (`phash_dist = 22`, `blockhash_dist = 32` for `checkerboard(4)`
        // vs `gradient`). Calibrated bounds keep a 2-bit safety margin
        // below observed to tolerate small cross-target jitter
        // (image_hasher's DCT uses f32 internally — Lanczos3 + blockhash
        // are integer-only and shouldn't vary, but phash might shift one
        // or two bits on a different target). If this assertion fails in
        // the 4-target determinism matrix, the underlying fingerprint
        // bundle byte-identity will ALSO fail in `tests/determinism.rs`
        // and that's the canonical signal — re-calibrate there.
        let checker = fingerprint_image(&checkerboard_png_bytes(4), &opts()).unwrap();
        let gradient = fingerprint_image(&gradient_png_bytes(), &opts()).unwrap();
        let checker_img = checker.image.as_ref().unwrap();
        let gradient_img = gradient.image.as_ref().unwrap();

        let phash_dist = hamming_distance_64(
            decode_hex_8(&checker_img.phash),
            decode_hex_8(&gradient_img.phash),
        );
        let blockhash_dist = hamming_distance_64(
            decode_hex_8(&checker_img.blockhash),
            decode_hex_8(&gradient_img.blockhash),
        );
        assert!(
            phash_dist >= 20,
            "phash Hamming distance between checkerboard + gradient was {phash_dist}; expected >= 20 (calibrated bound, observed 22 at E5 — re-calibrate if cross-target drift surfaces in tests/determinism.rs)"
        );
        assert!(
            blockhash_dist >= 30,
            "blockhash Hamming distance between checkerboard + gradient was {blockhash_dist}; expected >= 30 (calibrated bound, observed 32 at E5 — re-calibrate if cross-target drift surfaces in tests/determinism.rs)"
        );
    }

    #[test]
    fn fingerprint_image_rejects_non_image_bytes() {
        let opts = opts();
        // Random bytes that are not a valid image header.
        let bad = b"not an image, this is just plain text";
        let err = fingerprint_image(bad, &opts).unwrap_err();
        match err {
            AttestrumFingerprintError::ImageDecode(msg) => {
                assert!(
                    !msg.is_empty(),
                    "ImageDecode error should carry the underlying image-crate error message"
                );
            }
            other => panic!("expected ImageDecode, got {other:?}"),
        }
    }

    #[test]
    fn fingerprint_image_rejects_unrepresentable_epoch() {
        // Mirrors the text-path InvalidTimestamp test; image path goes
        // through the same jiff::Timestamp::from_second call.
        let png = checkerboard_png_bytes(4);
        let bad_opts = FingerprintOpts {
            source_date_epoch: i64::MAX,
        };
        let err = fingerprint_image(&png, &bad_opts).unwrap_err();
        match err {
            AttestrumFingerprintError::InvalidTimestamp(t) => assert_eq!(t, i64::MAX),
            other => panic!("expected InvalidTimestamp, got {other:?}"),
        }
    }

    #[test]
    fn fingerprint_image_bundle_round_trips_via_serde_json() {
        let png = checkerboard_png_bytes(4);
        let bundle = fingerprint_image(&png, &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        let back: FingerprintBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle, back);
    }

    #[test]
    fn fingerprint_image_bundle_serializes_camel_case_keys() {
        let png = checkerboard_png_bytes(4);
        let bundle = fingerprint_image(&png, &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        // ImageFingerprint nested fields under camelCase.
        assert!(json.contains("\"phash\""));
        assert!(json.contains("\"blockhash\""));
        assert!(json.contains("\"width\""));
        assert!(json.contains("\"height\""));
        // Modality serialised as Image (PascalCase default).
        assert!(json.contains("\"Image\""));
        // text field MUST be omitted (None + skip-if-None).
        assert!(!json.contains("\"text\""));
    }

    #[test]
    fn text_bundle_continues_to_omit_image_field() {
        // E2 added the image field; ensure E1's text-path bundles still
        // serialize without an "image" key (backwards-compat for
        // anyone who has serialized text bundles).
        let bundle = fingerprint_text(b"hello", &opts()).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(!json.contains("\"image\""));
    }

    // ========================================================================
    // E4: image-branch ISCC tests (image fixtures defined above)
    // ========================================================================

    #[test]
    fn fingerprint_image_populates_iscc_composition() {
        let bundle = fingerprint_image(&checkerboard_png_bytes(4), &opts()).unwrap();
        let iscc = bundle.iscc.as_ref().expect("image path must populate iscc");
        for (label, code) in [
            ("content_code", &iscc.content_code),
            ("data_code", &iscc.data_code),
            ("instance_code", &iscc.instance_code),
            ("composite", &iscc.composite),
        ] {
            assert!(
                code.starts_with("ISCC:"),
                "image-branch iscc.{label} = {code:?} must start with ISCC: prefix"
            );
            assert!(
                code.len() > "ISCC:".len(),
                "image-branch iscc.{label} must carry payload after the prefix"
            );
        }
    }

    #[test]
    fn fingerprint_image_iscc_is_deterministic() {
        let png = checkerboard_png_bytes(4);
        let a = fingerprint_image(&png, &opts()).unwrap();
        let b = fingerprint_image(&png, &opts()).unwrap();
        assert_eq!(
            a.iscc, b.iscc,
            "same encoded image bytes must yield identical iscc"
        );
    }

    #[test]
    fn fingerprint_image_distinct_content_distinct_composite() {
        let checker = fingerprint_image(&checkerboard_png_bytes(4), &opts()).unwrap();
        let gradient = fingerprint_image(&gradient_png_bytes(), &opts()).unwrap();
        let c_iscc = checker.iscc.as_ref().unwrap();
        let g_iscc = gradient.iscc.as_ref().unwrap();
        assert_ne!(
            c_iscc.composite, g_iscc.composite,
            "visually distinct images must yield distinct ISCC composites"
        );
        // Instance code (cryptographic hash over raw bytes) MUST differ.
        assert_ne!(c_iscc.instance_code, g_iscc.instance_code);
    }
}
