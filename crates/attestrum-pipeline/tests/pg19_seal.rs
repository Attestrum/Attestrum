//! Pulls the Tier-1 PG-19 seal generator's file-walking core
//! (`examples/pg19/seal.rs`) into `cargo test --workspace` so the six commit
//! gates (CLAUDE.md §7) cover the book-tree enumeration, the exact-bytes leaf
//! contract, and the seal determinism contract.
//!
//! The exhaustive cases — sorted walk + provenance mapping, error paths, and
//! seal-twice byte-identity over a fixture tree — live in `seal.rs`'s own
//! `#[cfg(test)]` block; included here via `#[path]` so they run as tests of
//! this crate (mirrors `tests/dolly_seal.rs`).

#[path = "../examples/pg19/seal.rs"]
mod seal;
