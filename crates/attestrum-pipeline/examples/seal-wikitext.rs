//! Lookback Phase A — programmatic WikiText-103 seal generator.
//!
//! Reads the `text` column of the local `wikitext-103-raw-v1` train Parquet
//! shards, segments + detokenizes them into one-leaf-per-passage corpus entries
//! (`wikitext/segment.rs`), and runs the deterministic [`build_corpus`] pipeline
//! to produce a CAS + `manifest.parquet` + Merkle root under the output dir.
//!
//! The corpus bytes are **detokenized to natural English** (see
//! `wikitext/segment.rs` and `docs/diagrams/lookback/wikitext-seal-pipeline.md`):
//! detokenization lives in segmentation, never in the PROTECTED
//! `attestrum-fingerprint` normalization (CLAUDE.md §4).
//!
//! Seal is **local** (Phase-0 topology decision); signing happens later in CI.
//! No network, no clock-derived sealed bytes — deterministic for a fixed input.
//!
//! ```text
//! cargo run -p attestrum-pipeline --example seal-wikitext -- <input-parquet-dir> <output-dir>
//! ```
//! Prints the Merkle root (64 lowercase hex + `\n`) to stdout; the leaf count,
//! byte total, and manifest path go to stderr.

// `seal.rs` references `crate::segment::…`, so both modules are declared here at
// the example's crate root (see the seal.rs module docs).
#[path = "wikitext/seal.rs"]
mod seal;
#[path = "wikitext/segment.rs"]
mod segment;

use std::env;
use std::path::PathBuf;
use std::process;

use attestrum_core::hex;

use seal::{passages_from_dir, passages_to_entries, seal};

fn main() {
    let mut args = env::args().skip(1);
    let (input_dir, output_dir) = match (args.next(), args.next()) {
        (Some(i), Some(o)) => (PathBuf::from(i), PathBuf::from(o)),
        _ => {
            eprintln!(
                "usage: cargo run -p attestrum-pipeline --example seal-wikitext \
                 -- <input-parquet-dir> <output-dir>"
            );
            process::exit(2);
        }
    };

    if let Err(e) = run(&input_dir, &output_dir) {
        eprintln!("seal-wikitext: {e}");
        process::exit(1);
    }
}

fn run(input_dir: &std::path::Path, output_dir: &std::path::Path) -> Result<(), seal::SealError> {
    let passages = passages_from_dir(input_dir)?;
    let entries = passages_to_entries(passages);
    eprintln!(
        "segmented {} passages from {}",
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
