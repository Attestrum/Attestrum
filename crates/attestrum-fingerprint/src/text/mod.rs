//! Sprint 5 S5-D1 E3: SimHash near-duplicate hash over the
//! already-PROTECTED-normalized text emitted by
//! [`attestrum_text_minhash::normalize_text`].
//!
//! The MinHash kernel (`normalize_text` + `minhash::compute`) was extracted to
//! the `attestrum-text-minhash` crate (§4, founder-approved 2026-06-06) so the
//! identical Rust compiles to `wasm32` for the attestrum.com near-match demo;
//! `fingerprint_text` now calls it from there. SimHash stays in-tree: it
//! consumes a `&str` already NFC-normalized + lowercased + whitespace-collapsed
//! to single ASCII spaces, tokenizes via `str::split(' ')` on that single-byte
//! delimiter, and operates over 5-gram word shingles.
//!
//! Visibility is `pub(crate)` — the public surface is the `simhash: u64` field
//! on [`super::TextFingerprint`], not the compute helper itself.
//!
//! # PROTECTED
//!
//! Per CLAUDE.md §4 and the E3 landing commit's `Protected-system-change:`
//! footer (2026-05-25), the [`simhash`] weighting scheme is locked: any change
//! to shingle size or SimHash weighting invalidates every previously-emitted
//! inclusion proof and requires a schema URI bump (`…/v0.1` → `…/v0.2`) with a
//! migration packet. (The MinHash parameters are locked in the
//! `attestrum-text-minhash` crate.)

pub(crate) mod simhash;
