//! `attestrum dedup` — deterministic intra-corpus near-duplicate rate.
//!
//! Answers "how much of this corpus is near-duplicated against itself?" — a
//! composition-quality signal in the same read-only family as
//! `attestrum decontaminate`. The output is a **read-only, unsigned** report
//! (`report.json` + `report.md`) that is a pure function of the corpus inputs.
//!
//! ## One determinism ruler — rides the PROTECTED kernel, does not fork it
//!
//! Normalization and the near-duplicate basis reuse
//! [`attestrum_text_minhash`] unchanged:
//! [`attestrum_text_minhash::normalize_text`] and
//! [`attestrum_text_minhash::minhash::compute`] (128-permutation, BLAKE3-keyed,
//! 5-gram word shingles). The near-dup basis is byte-identical to what
//! `attestrum index` / `attestrum prove` / `attestrum decontaminate` use. This
//! crate ships **no** second MinHash and modifies **no** §4 protected system.
//! Corpus ingestion reuses [`attestrum_decontaminate::ingest::read_corpus`].
//!
//! ## MinHash-LSH banding — bounded candidate generation
//!
//! Each 128-component signature is split into bands; documents sharing an
//! identical band become candidate pairs, which are then verified by the exact
//! MinHash Jaccard estimate and grouped into clusters by union-find. See
//! [`cluster`] for the algorithm and its tunables.
//!
//! ## Deferred (NOT in this leaf)
//!
//! Persisting per-leaf MinHash signatures in the sealed manifest — which would
//! let `dedup` skip recomputation and enable a cross-version near-dup *delta* —
//! is a §4 manifest-schema change (protected, needs approval + migration). This
//! leaf recomputes signatures from the raw corpus bytes each run.

pub mod cluster;
pub mod report;
