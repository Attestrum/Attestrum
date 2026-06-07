//! Pulls the Tier-1 dolly seal generator's Parquet-reading core
//! (`examples/dolly/seal.rs`) into `cargo test --workspace` so the six commit
//! gates (CLAUDE.md §7) cover the row-read-over-Parquet path and the seal
//! determinism contract.
//!
//! The exhaustive cases — provenance mapping and seal-twice byte-identity over a
//! fixture Parquet — live in `seal.rs`'s own `#[cfg(test)]` block; included here
//! via `#[path]` so they run as tests of this crate. `seal.rs` references
//! `crate::render::…`, so `render.rs` is declared alongside it (mirrors
//! `tests/dolly_render.rs`).

#[path = "../examples/dolly/render.rs"]
mod render;
#[path = "../examples/dolly/seal.rs"]
mod seal;
