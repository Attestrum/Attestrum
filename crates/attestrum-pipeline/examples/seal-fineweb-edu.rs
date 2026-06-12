//! Lookback — programmatic fineweb-edu seal generator (sharded-matrix rung).
//!
//! Reads the `text` / `id` / `language` columns of the fineweb-edu Parquet
//! shard(s) in the input directory and runs the deterministic [`build_corpus`]
//! pipeline to produce a CAS + `manifest.parquet` + Merkle root under the
//! output dir.
//!
//! **One row = one leaf, sealed as its exact `text` column bytes** — no
//! render, no normalization, no added newline
//! (`docs/diagrams/lookback/fineweb10bt-seal-pipeline.md`). `source_uri` is
//! the row's own upstream `id` (`urn:uuid`), so the leaf set is invariant
//! under sharding: in CI each of the 14 matrix jobs runs this generator on
//! its one downloaded shard file, and `attestrum merge` combines the shard
//! manifests into the canonical root. The 100BT rung reuses this generator
//! unchanged with different inputs. The PROTECTED `attestrum-fingerprint`
//! normalization (CLAUDE.md §4) is untouched.
//!
//! Seal runs in CI (the corpus outsizes a laptop); signing happens in a later
//! workflow phase. No network, no clock-derived sealed bytes — deterministic
//! for a fixed input.
//!
//! ```text
//! cargo run -p attestrum-pipeline --example seal-fineweb-edu -- <input-parquet-dir> <output-dir>
//! ```
//! Prints the Merkle root (64 lowercase hex + `\n`) to stdout; the leaf count,
//! byte total, and manifest path go to stderr.

#[path = "fineweb_edu/seal.rs"]
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
                "usage: cargo run -p attestrum-pipeline --example seal-fineweb-edu \
                 -- <input-parquet-dir> <output-dir>"
            );
            process::exit(2);
        }
    };

    if let Err(e) = run(&input_dir, &output_dir) {
        eprintln!("seal-fineweb-edu: {e}");
        process::exit(1);
    }
}

fn run(input_dir: &std::path::Path, output_dir: &std::path::Path) -> Result<(), seal::SealError> {
    let rows = rows_from_dir(input_dir)?;
    let entries = rows_to_entries(rows);
    eprintln!("read {} rows from {}", entries.len(), input_dir.display());

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
