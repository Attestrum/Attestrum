//! `attestrum remove` — emit unsigned two-manifest removal evidence.
//!
//! Read-only: it proves the target document was included in `--before` and is
//! absent from `--after` (reusing `attestrum_prove::prove()` in both
//! directions, unsigned), then writes a `report.json` + `report.md` bundling
//! the two in-toto Statements. Mints no new predicate and touches no manifest.

use attestrum_remove::evidence::{build_removal, RemoveError};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug)]
pub struct Args {
    pub before: PathBuf,
    pub after: PathBuf,
    pub target: String,
    pub source_date_epoch: i64,
    pub out: PathBuf,
}

#[derive(Debug, Error)]
pub enum RemoveCliError {
    #[error("--target {0:?} must be a 64-char lowercase BLAKE3 hex digest")]
    BadTarget(String),

    #[error(transparent)]
    Removal(#[from] RemoveError),

    #[error("serializing report.json")]
    Serialize(#[source] serde_json::Error),

    #[error("creating output directory {path}")]
    CreateOut {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("writing {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn run(args: Args) -> Result<(), RemoveCliError> {
    let target = parse_blake3_hex(&args.target)
        .ok_or_else(|| RemoveCliError::BadTarget(args.target.clone()))?;

    let report = build_removal(target, &args.before, &args.after, args.source_date_epoch)?;
    let json = report.to_json().map_err(RemoveCliError::Serialize)?;
    let markdown = report.to_markdown();

    std::fs::create_dir_all(&args.out).map_err(|source| RemoveCliError::CreateOut {
        path: args.out.clone(),
        source,
    })?;
    let json_path = args.out.join("report.json");
    let md_path = args.out.join("report.md");
    std::fs::write(&json_path, json.as_bytes()).map_err(|source| RemoveCliError::Write {
        path: json_path.clone(),
        source,
    })?;
    std::fs::write(&md_path, markdown.as_bytes()).map_err(|source| RemoveCliError::Write {
        path: md_path.clone(),
        source,
    })?;

    println!(
        "remove: target {} proved removed (included in --before, absent from --after)",
        report.target
    );
    println!("  {}", json_path.display());
    println!("  {}", md_path.display());
    Ok(())
}

/// Parse a 64-char lowercase BLAKE3 hex digest into 32 bytes; `None` otherwise.
/// Mirrors the exact-hash arm of `attestrum prove`'s target parsing.
fn parse_blake3_hex(arg: &str) -> Option<[u8; 32]> {
    if arg.len() != 64
        || !arg
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in arg.as_bytes().chunks_exact(2).enumerate() {
        bytes[i] = (hex_nibble(chunk[0]) << 4) | hex_nibble(chunk[1]);
    }
    Some(bytes)
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => unreachable!("charset pre-validated by parse_blake3_hex"),
    }
}
