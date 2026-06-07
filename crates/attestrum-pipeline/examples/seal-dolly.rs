//! Lookback Tier-1 — programmatic databricks-dolly-15k seal generator.
//!
//! Reads the `instruction` / `context` / `response` columns of the local
//! dolly-15k train Parquet shard(s), renders each row to natural text
//! (`dolly/render.rs`), and runs the deterministic [`build_corpus`] pipeline to
//! produce a CAS + `manifest.parquet` + Merkle root under the output dir.
//!
//! The corpus bytes are the rendered rows (`instruction` / optional `context` /
//! `response`, blank-line separated; see `dolly/render.rs` and
//! `docs/diagrams/lookback/dolly-seal-pipeline.md`). Unlike WikiText-103-raw there
//! is no detokenization — dolly is already natural English; the PROTECTED
//! `attestrum-fingerprint` normalization (CLAUDE.md §4) is untouched.
//!
//! Seal is **local**; signing happens later in CI. No network, no clock-derived
//! sealed bytes — deterministic for a fixed input.
//!
//! ```text
//! cargo run -p attestrum-pipeline --example seal-dolly -- <input-parquet-dir> <output-dir>
//! ```
//! Prints the Merkle root (64 lowercase hex + `\n`) to stdout; the leaf count,
//! byte total, and manifest path go to stderr.

// `seal.rs` references `crate::render::…`, so both modules are declared here at
// the example's crate root (mirrors seal-wikitext.rs).
#[path = "dolly/render.rs"]
mod render;
#[path = "dolly/seal.rs"]
mod seal;

use std::env;
use std::path::PathBuf;
use std::process;

use attestrum_core::hex;

use seal::{rows_from_dir, rows_to_entries, seal};

fn main() {
    let mut args = env::args().skip(1);
    let (input_dir, output_dir) = match (args.next(), args.next()) {
        (Some(i), Some(o)) => (PathBuf::from(i), PathBuf::from(o)),
        _ => {
            eprintln!(
                "usage: cargo run -p attestrum-pipeline --example seal-dolly \
                 -- <input-parquet-dir> <output-dir>"
            );
            process::exit(2);
        }
    };

    if let Err(e) = run(&input_dir, &output_dir) {
        eprintln!("seal-dolly: {e}");
        process::exit(1);
    }
}

fn run(input_dir: &std::path::Path, output_dir: &std::path::Path) -> Result<(), seal::SealError> {
    let rows = rows_from_dir(input_dir)?;
    let entries = rows_to_entries(rows);
    eprintln!(
        "rendered {} rows from {}",
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
