//! `attestrum compose` — deterministic corpus-composition summary.
//!
//! Answers "what is this corpus *made of*?" — the EU AI Act Article 53(1)(d)
//! training-content summary surface, expressed as the language / source-type /
//! SPDX-license / modality mix of a sealed corpus. The output is a
//! **read-only, unsigned** report (`report.json` + `report.md`) that is a pure
//! function of the sealed manifest: the same manifest produces byte-identical
//! reports on any machine.
//!
//! ## Pure manifest read — touches no §4 protected system
//!
//! [`aggregate::aggregate_manifest`] streams the manifest with
//! [`attestrum_manifest::ManifestBatchReader`] (constant-memory, ≤8192-row
//! batches — the same reader `attestrum diff` uses) and walks every
//! [`attestrum_manifest::ManifestEntry`]. Every field it needs is already
//! persisted in the sealed 18-column manifest: `modality`, `source_type`,
//! `license_spdx`, `language`, `size_bytes`, `included`. It computes nothing
//! new about the documents and consumes no fingerprint kernel.
//!
//! ## The Merkle anchor matches the seal
//!
//! Aggregation collects every row's `document_id` in on-disk canonical order
//! and recomputes the corpus root via [`attestrum_merkle::merkle_root`] —
//! byte-identical to the root the build pipeline sealed (`attestrum-pipeline`
//! feeds the same leaves, all rows, same order). The summary is therefore tied
//! to a specific, verifiable corpus state.
//!
//! ## Honesty — `unspecified` is a bucket, never a silent drop
//!
//! `modality` is always present; `source_type`, `license_spdx`, and `language`
//! are optional. A `None` value folds into an explicit `"unspecified"` bucket
//! and is excluded from the dimension's coverage count, and the report carries
//! a coverage % per dimension (by document count and by bytes). Composition is
//! aggregated over `included == true` rows (the actual training content);
//! recorded-but-excluded rows are reported as a separate count.
//!
//! ## Deferred (NOT in this leaf)
//!
//! A signed `composition` predicate and emitting the Commission's official
//! Article 53 template are §4 / §A4 work — a new predicate URI (and, if a
//! third party validates the template, a §2.5 CI validator gate) requiring the
//! high-stakes-decision protocol + founder approval. This leaf emits a plain
//! unsigned report only.

pub mod aggregate;
pub mod report;
