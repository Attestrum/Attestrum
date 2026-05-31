//! Pulls the Lookback seal generator's Parquet-reading core (`examples/wikitext/
//! seal.rs`) into `cargo test --workspace` so the six commit gates (CLAUDE.md §7)
//! cover the segmentation-over-Parquet path and the seal determinism contract.
//!
//! The exhaustive cases — cross-shard segmentation, fixed provenance, and
//! seal-twice byte-identity over a fixture Parquet — live in `seal.rs`'s own
//! `#[cfg(test)]` block; included here via `#[path]` so they run as tests of this
//! crate. `seal.rs` references `crate::segment::…`, so `segment.rs` is declared
//! alongside it (mirrors `tests/wikitext_segment.rs`).

#[path = "../examples/wikitext/seal.rs"]
mod seal;
#[path = "../examples/wikitext/segment.rs"]
mod segment;
