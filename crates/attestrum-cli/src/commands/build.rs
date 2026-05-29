//! `attestrum build` — corpus.toml → sealed manifest.
//!
//! Loads a corpus.toml per BUILD-PLAN §8.3, materialises a
//! `Vec<attestrum_pipeline::CorpusEntry>` (resolving each `source_url` to a
//! local file in v1 — `http://` / `https://` is deferred to the
//! Sprint 4 fetch layer), calls `attestrum_pipeline::build_corpus`, and
//! prints a structured stdout summary suitable for piping. Matches the
//! `docs/diagrams/sprint-3/attestrum-build-cli.md` sequence.

use std::fs;
use std::path::{Path, PathBuf};

use attestrum_cas::CasStore;
use attestrum_core::{BuildContext, Modality, SourceType};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, ContentSource, CorpusEntry};
use serde::Deserialize;
use thiserror::Error;

/// Subcommand arguments. Owned by `main` and passed in by value.
#[derive(Debug)]
pub struct Args {
    pub corpus: PathBuf,
    pub workspace: PathBuf,
    pub source_date_epoch: Option<i64>,
    pub offline: bool,
}

/// Errors `build::run` can surface. Each variant maps to exit code 1
/// (the main binary's catch-all for runtime errors). Clap-native parse
/// failures are exit 2 (clap exits before `run` is called).
#[derive(Debug, Error)]
pub enum BuildCliError {
    #[error("corpus file not found: {0}")]
    CorpusMissing(PathBuf),

    #[error("corpus file read failed at {path}: {source}")]
    CorpusRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("corpus.toml parse failed at {path}: {source}")]
    CorpusParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("unsupported source_url scheme in entry {ordinal}: {url} — only file:// and local paths are supported in v1")]
    UnsupportedScheme { ordinal: usize, url: String },

    #[error("workspace prepare failed at {path}: {source}")]
    Workspace {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("pipeline build failed: {0}")]
    Pipeline(#[from] attestrum_pipeline::BuildError),
}

/// `attestrum build` entry point. Returns `Ok(())` on success; main turns
/// `Err` into exit code 1 with a chained-source message on stderr.
pub fn run(args: Args) -> Result<(), BuildCliError> {
    // `--offline` is a no-op in v1; documented in the diagram. Acknowledge
    // it via a tracing message so operators can see it was observed.
    if args.offline {
        tracing::info!("--offline acknowledged (no-op in v1; pipeline does no network fetches)");
    }

    if !args.corpus.exists() {
        return Err(BuildCliError::CorpusMissing(args.corpus.clone()));
    }
    let raw = fs::read_to_string(&args.corpus).map_err(|source| BuildCliError::CorpusRead {
        path: args.corpus.clone(),
        source,
    })?;
    let config: CorpusConfig =
        toml::from_str(&raw).map_err(|source| BuildCliError::CorpusParse {
            path: args.corpus.clone(),
            source,
        })?;

    // Precedence for source_date_epoch: CLI flag > [corpus] toml > 0.
    let effective_epoch = args
        .source_date_epoch
        .or(config.corpus.source_date_epoch)
        .unwrap_or(0);

    // Resolve entries: per-entry fetched_at defaults to the corpus
    // epoch; source_url is resolved to a local Path (http(s) rejected).
    let corpus_dir = args.corpus.parent().unwrap_or_else(|| Path::new("."));
    let entries: Vec<CorpusEntry> = config
        .entries
        .into_iter()
        .enumerate()
        .map(|(ordinal, e)| materialise_entry(ordinal, e, corpus_dir, effective_epoch))
        .collect::<Result<_, _>>()?;

    let ctx = BuildContext::new(args.workspace.clone(), effective_epoch);
    let cas_root = args.workspace.join(".attestrum");
    fs::create_dir_all(&cas_root).map_err(|source| BuildCliError::Workspace {
        path: cas_root.clone(),
        source,
    })?;
    let cas = CasStore::new(&cas_root).map_err(|source| BuildCliError::Workspace {
        path: cas_root.clone(),
        source,
    })?;
    let manifest_dir = cas_root.join("manifests");

    tracing::info!(
        corpus = %args.corpus.display(),
        workspace = %args.workspace.display(),
        leaves = entries.len(),
        source_date_epoch = effective_epoch,
        "running attestrum build"
    );

    let output = build_corpus(&ctx, &cas, &entries, &manifest_dir)?;

    // Sibling artifact to manifest.parquet so `attestrum publish` can commit
    // the corpus's Merkle root verbatim under `attestrum/merkle.root` (per
    // docs/diagrams/overview/hub-publish.md). Format: 64 lowercase hex chars +
    // trailing newline (matches the stdout summary line and is `git diff`-
    // friendly). Visitors who `git clone` a published dataset can re-derive
    // the root from the manifest and string-compare against this file.
    let merkle_root_path = manifest_dir.join("merkle.root");
    let merkle_root_text = format!("{}\n", hex_64(&output.merkle_root));
    fs::write(&merkle_root_path, &merkle_root_text).map_err(|source| BuildCliError::Workspace {
        path: merkle_root_path.clone(),
        source,
    })?;

    print_summary(&output, &merkle_root_path);
    Ok(())
}

fn materialise_entry(
    ordinal: usize,
    e: EntryConfig,
    corpus_dir: &Path,
    epoch_default: i64,
) -> Result<CorpusEntry, BuildCliError> {
    let path = resolve_local_path(ordinal, &e.source_url, corpus_dir)?;
    Ok(CorpusEntry {
        source_uri: e.source_url,
        content: ContentSource::Path(path),
        modality: e.modality.into(),
        mime_type: e.mime_type,
        source_type: e.source_type.map(SourceTypeToml::into),
        source_dataset_id: e.source_dataset_id,
        registered_domain: e.registered_domain,
        license_spdx: e.license_spdx,
        language: e.language,
        fetched_at: Some(e.fetched_at.unwrap_or(epoch_default)),
        signals: ManifestSignals::default(),
        included: e.included.unwrap_or(true),
        exclusion_reason: e.exclusion_reason,
    })
}

fn resolve_local_path(
    ordinal: usize,
    source_url: &str,
    corpus_dir: &Path,
) -> Result<PathBuf, BuildCliError> {
    if let Some(rest) = source_url.strip_prefix("file://") {
        return Ok(PathBuf::from(rest));
    }
    if source_url.starts_with("http://") || source_url.starts_with("https://") {
        return Err(BuildCliError::UnsupportedScheme {
            ordinal,
            url: source_url.to_string(),
        });
    }
    // Plain path: relative to corpus.toml's parent dir for portability,
    // absolute paths used as-is.
    let p = PathBuf::from(source_url);
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(corpus_dir.join(p))
    }
}

fn print_summary(output: &attestrum_pipeline::BuildOutput, merkle_root_path: &Path) {
    println!("attestrum build: ok");
    println!("  merkle_root:  {}", hex_64(&output.merkle_root));
    println!("  manifest:     {}", output.manifest_path.display());
    println!("  merkle_file:  {}", merkle_root_path.display());
    println!("  leaf_count:   {}", output.leaf_count);
    println!("  total_bytes:  {}", output.total_bytes);
}

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

// ============================================================================
// corpus.toml schema
// ============================================================================

/// Top-level `corpus.toml` shape. Single `[corpus]` section + a sequence
/// of `[[entry]]` tables.
#[derive(Debug, Deserialize)]
struct CorpusConfig {
    #[serde(default)]
    corpus: CorpusMeta,
    #[serde(default, rename = "entry")]
    entries: Vec<EntryConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct CorpusMeta {
    #[allow(dead_code)] // Captured for future surfacing in summary output.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    source_date_epoch: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct EntryConfig {
    source_url: String,
    modality: ModalityToml,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    source_type: Option<SourceTypeToml>,
    #[serde(default)]
    source_dataset_id: Option<String>,
    #[serde(default)]
    registered_domain: Option<String>,
    #[serde(default)]
    license_spdx: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    fetched_at: Option<i64>,
    #[serde(default)]
    included: Option<bool>,
    #[serde(default)]
    exclusion_reason: Option<String>,
}

// Wrapper enums with snake_case serde so corpus.toml reads as
// `modality = "text"` and `source_type = "public_dataset"` instead of
// `attestrum-core`'s default PascalCase serialisation (which is kept
// untouched so the manifest schema/round-trip tests don't break).

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ModalityToml {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
    Other,
}

impl From<ModalityToml> for Modality {
    fn from(m: ModalityToml) -> Self {
        match m {
            ModalityToml::Text => Modality::Text,
            ModalityToml::Image => Modality::Image,
            ModalityToml::Audio => Modality::Audio,
            ModalityToml::Video => Modality::Video,
            ModalityToml::Pdf => Modality::Pdf,
            ModalityToml::Other => Modality::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceTypeToml {
    Crawl,
    PublicDataset,
    PrivateLicensed,
    User,
    Synthetic,
    Other,
}

impl From<SourceTypeToml> for SourceType {
    fn from(s: SourceTypeToml) -> Self {
        match s {
            SourceTypeToml::Crawl => SourceType::Crawl,
            SourceTypeToml::PublicDataset => SourceType::PublicDataset,
            SourceTypeToml::PrivateLicensed => SourceType::PrivateLicensed,
            SourceTypeToml::User => SourceType::User,
            SourceTypeToml::Synthetic => SourceType::Synthetic,
            SourceTypeToml::Other => SourceType::Other,
        }
    }
}
