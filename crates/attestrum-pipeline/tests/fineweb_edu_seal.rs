//! Pulls the fineweb-edu seal generator's Parquet-reading core
//! (`examples/fineweb_edu/seal.rs`) into `cargo test --workspace` so the six
//! commit gates (CLAUDE.md §7) cover the row-read-over-Parquet path (zstd
//! fixtures), the exact-text-bytes leaf contract, the seal determinism
//! contract, and the split-vs-whole merged-root equality the 14-shard CI
//! matrix relies on.
//!
//! The cases live in `seal.rs`'s own `#[cfg(test)]` block; included here via
//! `#[path]` so they run as tests of this crate (examples are not test-gated
//! by default — same arrangement as the dolly and pg19 seal generators).

#[path = "../examples/fineweb_edu/seal.rs"]
mod seal;
