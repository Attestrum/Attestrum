//! `attestrum compose` — emit an unsigned corpus-composition summary.
//!
//! Read-only and network-free: it streams a sealed `manifest.parquet`, computes
//! the language / source-type / SPDX-license / modality mix (weighted by both
//! document count and bytes, with per-dimension coverage), recomputes the
//! corpus Merkle root as an anchor, and writes `report.json` + `report.md`.
//! Modifies no manifest and emits no signed predicate.

use attestrum_compose::{aggregate::aggregate_manifest, report};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug)]
pub struct Args {
    pub manifest: PathBuf,
    pub out: PathBuf,
}

#[derive(Debug, Error)]
pub enum ComposeCliError {
    #[error("reading manifest {path}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: attestrum_core::AttestrumError,
    },

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

pub fn run(args: Args) -> Result<(), ComposeCliError> {
    let comp = aggregate_manifest(&args.manifest).map_err(|source| ComposeCliError::Manifest {
        path: args.manifest.clone(),
        source,
    })?;

    let report = report::build(args.manifest.display().to_string(), &comp, None);
    let json = report.to_json().map_err(ComposeCliError::Serialize)?;
    let markdown = report.to_markdown();

    std::fs::create_dir_all(&args.out).map_err(|source| ComposeCliError::CreateOut {
        path: args.out.clone(),
        source,
    })?;
    let json_path = args.out.join("report.json");
    let md_path = args.out.join("report.md");
    std::fs::write(&json_path, json.as_bytes()).map_err(|source| ComposeCliError::Write {
        path: json_path.clone(),
        source,
    })?;
    std::fs::write(&md_path, markdown.as_bytes()).map_err(|source| ComposeCliError::Write {
        path: md_path.clone(),
        source,
    })?;

    println!(
        "compose: {} document(s) ({} included · {} excluded) → {} modality / {} language bucket(s)",
        comp.total_documents,
        comp.included_documents,
        comp.excluded_documents,
        comp.modality.buckets.len(),
        comp.language.buckets.len()
    );
    println!("  {}", json_path.display());
    println!("  {}", md_path.display());
    Ok(())
}
