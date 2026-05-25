//! Sprint 5 S5-D1 E3: hand-rolled near-duplicate-detection hashes over the
//! already-PROTECTED-normalized text emitted by [`super::normalize_text`].
//!
//! Per PATH-A-BRIEF Part 2.1 line 522, MinHash + SimHash are implemented
//! in-tree (no external crate). Both submodules consume a `&str` that is
//! already NFC-normalized + lowercased + whitespace-collapsed to single
//! ASCII spaces (the locked invariants of `normalize_text` at lib.rs §
//! PROTECTED), so each tokenizes via `str::split(' ')` on that single-byte
//! delimiter and operates over 5-gram word shingles.
//!
//! Visibility is `pub(crate)` — the public surface change at E3 is the two
//! new fields on [`super::TextFingerprint`] (`minhash: Vec<u64>` +
//! `simhash: u64`), not the compute helpers themselves. Downstream
//! `attestrum-prove` (Sprint 5 E9) reads the fields off the struct and
//! computes Jaccard / Hamming distances via plain Rust ops; it does not
//! need to call into this module.
//!
//! # PROTECTED
//!
//! Per CLAUDE.md §4 and founder approval recorded in the E3 landing
//! commit's `Protected-system-change:` footer (2026-05-25), the algorithm
//! parameters in [`minhash`] and [`simhash`] are locked. Once a single
//! `MatchEvidence::MinHash` inclusion proof emits citing
//! `https://attestrum.com/fingerprint/v0.1`, any change to shingle size,
//! permutation count, BLAKE3 keying scheme, or SimHash weighting
//! invalidates every previously-emitted inclusion proof and requires a
//! schema URI bump (`…/v0.1` → `…/v0.2`) with a migration packet.

pub(crate) mod minhash;
pub(crate) mod simhash;
