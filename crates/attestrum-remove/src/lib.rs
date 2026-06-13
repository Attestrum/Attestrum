//! `attestrum remove` — deterministic two-manifest removal evidence.
//!
//! Answers "can I prove this document was *removed* between two sealed corpus
//! versions?" — the takedown / removal question. The output is a
//! **read-only, unsigned** report (`report.json` + `report.md`) bundling two
//! cryptographic proofs over two already-sealed manifests.
//!
//! ## Reuses `attestrum_prove::prove()` — changes no protected system
//!
//! The target (a BLAKE3 `document_id`) is proved twice, both with `sign=false`:
//! **inclusion** against `--before` (the document *was* there, with an RFC-6962
//! audit path) and **non-inclusion** against `--after` (the document *is gone*,
//! via the sorted-Merkle adjacent-leaf proof). Both predicate types
//! (`inclusion-proof/v0.3`, `non-inclusion-proof/v0.3`) and the PROTECTED
//! `attestrum-merkle` audit paths are consumed read-only — `remove` mints no new
//! predicate URI and touches no §4 schema. See [`evidence`] for the flow.
//!
//! ## Deferred (NOT in this leaf — the big one)
//!
//! A signed `takedown` predicate (a new §4 URI), the append-only
//! `attestrum-ledger` (a stub today), and a corpus-chain `prev_merkle_root`
//! field (a §4 predicate bump) are all §4 / §A4 high-stakes work requiring the
//! high-stakes-decision protocol + founder approval. This leaf bundles two
//! existing read-only proofs into an unsigned report and stops there.

pub mod evidence;
pub mod report;
