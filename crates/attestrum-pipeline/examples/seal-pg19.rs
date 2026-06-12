//! Lookback Tier-1 — programmatic deepmind-pg19 seal generator.
//!
//! Walks the PG-19 book tree (`<input-dir>/{train,validation,test}/*.txt`, one
//! plain-text file per book) and runs the deterministic [`build_corpus`]
//! pipeline to produce a CAS + `manifest.parquet` + Merkle root under the
//! output dir.
//!
//! **One file = one leaf, sealed as its exact bytes** — no render, no
//! normalization (`docs/diagrams/lookback/pg19-seal-pipeline.md`). Entries are
//! `ContentSource::Path`, so the ~11.5 GB corpus never sits in memory; the
//! PROTECTED `attestrum-fingerprint` normalization (CLAUDE.md §4) is untouched.
//!
//! Seal runs in CI (the corpus outsizes a laptop); signing happens in a later
//! workflow phase. No network, no clock-derived sealed bytes — deterministic
//! for a fixed input tree.
//!
//! ```text
//! cargo run -p attestrum-pipeline --example seal-pg19 -- <input-dir> <output-dir>
//! ```
//! Prints the Merkle root (64 lowercase hex + `\n`) to stdout; the leaf count,
//! byte total, and manifest path go to stderr.

#[path = "pg19/seal.rs"]
mod seal;

use std::env;
use std::path::PathBuf;
use std::process;

use attestrum_core::hex;

use seal::{book_paths, paths_to_entries, seal};

fn main() {
    let mut args = env::args().skip(1);
    let (input_dir, output_dir) = match (args.next(), args.next()) {
        (Some(i), Some(o)) => (PathBuf::from(i), PathBuf::from(o)),
        _ => {
            eprintln!(
                "usage: cargo run -p attestrum-pipeline --example seal-pg19 \
                 -- <input-dir> <output-dir>"
            );
            process::exit(2);
        }
    };

    if let Err(e) = run(&input_dir, &output_dir) {
        eprintln!("seal-pg19: {e}");
        process::exit(1);
    }
}

fn run(input_dir: &std::path::Path, output_dir: &std::path::Path) -> Result<(), seal::SealError> {
    let books = book_paths(input_dir)?;
    let entries = paths_to_entries(input_dir, books);
    eprintln!(
        "enumerated {} book files from {}",
        entries.len(),
        input_dir.display()
    );

    let output = seal(&entries, output_dir, 0)?;

    println!("{}", hex::encode_32(&output.merkle_root));
    eprintln!(
        "leaves={} bytes={} manifest={}",
        output.leaf_count,
        output.total_bytes,
        output.manifest_path.display()
    );
    Ok(())
}
