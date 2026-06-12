//! `attestrum decontaminate` — deterministic benchmark-contamination scan.
//!
//! Answers "did evaluation-benchmark questions leak into this corpus?" — a
//! corpus-composition question in the same family as inclusion / non-inclusion.
//! The output is a **read-only, unsigned** report (`report.json` +
//! `report.md`) that is a pure function of the corpus and benchmark inputs:
//! the same inputs produce byte-identical reports on any machine.
//!
//! ## One determinism ruler — rides the PROTECTED kernel, does not fork it
//!
//! Text normalization and the near-duplicate signal reuse
//! [`attestrum_text_minhash`] unchanged: [`attestrum_text_minhash::normalize_text`]
//! (NFC → lowercase → whitespace-collapse) and
//! [`attestrum_text_minhash::minhash::compute`] (128-permutation, BLAKE3-keyed,
//! 5-gram word shingles). The contamination near-dup basis is therefore
//! byte-identical to what `attestrum index` / `attestrum prove` use — one ruler
//! everywhere. This crate adds **no** second MinHash and modifies **no** §4
//! protected system; it consumes the kernel read-only.
//!
//! ## Three signals (any one fires → a hit)
//!
//! - **exact** — the document and the benchmark item share ≥ 1 [`EXACT_N`]-gram.
//! - **near** — MinHash Jaccard ≥ `near_threshold` (default
//!   [`DEFAULT_NEAR_THRESHOLD`]).
//! - **contained** — ≥ [`DEFAULT_CONTAINMENT_THRESHOLD`] of the item's
//!   [`NEAR_N`]-gram shingles appear in the document (catches an answer buried
//!   in filler, where Jaccard is diluted below threshold).
//!
//! The exact and containment signals need raw shingle *sets*, which the kernel
//! does not expose, so [`shingle`] adds a small BLAKE3 shingle-set helper —
//! additive, no `xxhash` dependency.

pub mod detect;
pub mod ingest;
pub mod report;
pub mod shingle;

/// Word-count of an exact-match shingle. A shared 13-gram is the classic
/// verbatim-overlap signal (traces to GPT-3 contamination checks).
pub const EXACT_N: usize = 13;

/// Word-count of a near-duplicate / containment shingle. Matches the PROTECTED
/// kernel's 5-gram shingle width so the containment denominator and the MinHash
/// signal reason over the same granularity.
pub const NEAR_N: usize = 5;

/// Default MinHash Jaccard threshold for the `near` signal.
pub const DEFAULT_NEAR_THRESHOLD: f64 = 0.80;

/// Default fraction-of-item-shingles threshold for the `contained` signal.
pub const DEFAULT_CONTAINMENT_THRESHOLD: f64 = 0.90;

/// Maximum characters of normalized item text retained for the report snippet.
pub const SNIPPET_CHARS: usize = 120;
