//! `attestrum index build` — derive the fuzzy-lookup sidecar indexes for a
//! sealed corpus workspace.
//!
//! Reads `<workspace>/.attestrum/manifests/manifest.parquet` + the CAS at
//! `<workspace>/.attestrum/` and writes the MinHash / perceptual / ISCC
//! sidecars under `<workspace>/.attestrum/index/<kind>/v1.idx`. The indexes are
//! discovery-grade acceleration for `attestrum prove`'s fuzzy paths — derived,
//! unsigned, and rebuildable from the manifest + CAS at any time.

use std::path::PathBuf;

use thiserror::Error;

/// Subcommand arguments. Owned by `main` and passed in by value.
#[derive(Debug)]
pub struct Args {
    /// Workspace whose `.attestrum/` holds the sealed manifest + CAS.
    pub workspace: PathBuf,
    /// Reproducible Builds timestamp (epoch seconds) for fingerprinting.
    pub source_date_epoch: Option<i64>,
}

/// Errors `index::run` can surface (all map to exit code 1).
#[derive(Debug, Error)]
pub enum IndexCliError {
    /// No sealed manifest at the expected workspace sub-path.
    #[error("sealed manifest not found at {0} — run `attestrum build` first")]
    ManifestMissing(PathBuf),

    /// The index builder failed (manifest read, CAS read, or atomic write).
    #[error("index build failed: {0}")]
    Build(#[from] attestrum_index::error::IndexError),
}

/// `attestrum index build` entry point. Returns `Ok(())` on success; `main`
/// turns `Err` into exit code 1 with a chained-source message on stderr.
pub fn run(args: Args) -> Result<(), IndexCliError> {
    let cas_root = args.workspace.join(".attestrum");
    let manifest = cas_root.join("manifests").join("manifest.parquet");
    if !manifest.exists() {
        return Err(IndexCliError::ManifestMissing(manifest));
    }
    let sde = args.source_date_epoch.unwrap_or(0);

    tracing::info!(
        workspace = %args.workspace.display(),
        source_date_epoch = sde,
        "building fuzzy-lookup sidecar indexes"
    );

    let report = attestrum_index::build::build_all(&manifest, &cas_root, sde)?;

    println!("attestrum index build: ok");
    println!(
        "  minhash:     {} leaves, {} buckets",
        report.minhash.leaves, report.minhash.buckets
    );
    println!(
        "  perceptual:  {} leaves, {} buckets",
        report.perceptual.leaves, report.perceptual.buckets
    );
    println!(
        "  iscc:        {} leaves, {} buckets",
        report.iscc.leaves, report.iscc.buckets
    );
    println!("  index dir:   {}", cas_root.join("index").display());
    Ok(())
}
