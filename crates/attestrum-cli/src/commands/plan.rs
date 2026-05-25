//! `attestrum plan --corpus <corpus.toml> --shards N --out <dir>` —
//! deterministic shard partitioning for sub-corpus builds. Reads a
//! corpus.toml, computes a shard id per entry from its `source_url`,
//! and writes one `shard-NNNN.toml` file per non-empty shard under
//! `<dir>`. Each shard file is a self-contained corpus.toml that
//! `attestrum build` consumes directly.
//!
//! See `docs/diagrams/sprint-3/sharding.md` for the deterministic
//! shard-assignment contract and the merge round-trip semantics.
//!
//! **Determinism**: `shard_id = (BLAKE3(source_url.as_bytes()) first 8
//! bytes interpreted as little-endian u64) mod N`. Depends only on the
//! source_url string (not file content, not entry order); re-running
//! `attestrum plan` with the same `--corpus` and `--shards` produces
//! byte-identical shard files. Duplicate `source_url` entries always
//! co-locate to the same shard.
//!
//! **Passthrough**: this module reads corpus.toml as a generic
//! `toml::Value` rather than the typed `CorpusConfig` from
//! `commands::build`, so unknown / future-additions fields pass through
//! to each shard file without being silently dropped. The input is
//! assumed to be a valid corpus.toml — `attestrum build` re-validates per
//! shard, surfacing any structural issue at that stage.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use thiserror::Error;
use toml::Value;

/// Subcommand arguments.
#[derive(Debug)]
pub struct Args {
    pub corpus: PathBuf,
    pub shards: u32,
    pub out: PathBuf,
}

/// Errors `plan::run` can surface. All map to exit code 1.
#[derive(Debug, Error)]
pub enum PlanError {
    #[error("--shards must be >= 1; got {0}")]
    InvalidShardCount(u32),

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

    #[error("corpus.toml [[entry]] {ordinal} missing required string field `source_url`")]
    EntryMissingSourceUrl { ordinal: usize },

    #[error("output dir prepare failed at {path}: {source}")]
    OutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("shard file emit failed at {path}: {source}")]
    EmitShard {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("shard file serialize failed: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// `attestrum plan` entry point. Returns 0 on success, 1 on any error.
/// All errors are printed to stderr inside this function.
pub fn run(args: Args) -> u8 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("attestrum plan: {err}");
            let mut source = std::error::Error::source(&err);
            while let Some(s) = source {
                eprintln!("  caused by: {s}");
                source = std::error::Error::source(s);
            }
            1
        }
    }
}

fn run_inner(args: Args) -> Result<(), PlanError> {
    if args.shards == 0 {
        return Err(PlanError::InvalidShardCount(args.shards));
    }
    if !args.corpus.exists() {
        return Err(PlanError::CorpusMissing(args.corpus.clone()));
    }
    let raw = fs::read_to_string(&args.corpus).map_err(|source| PlanError::CorpusRead {
        path: args.corpus.clone(),
        source,
    })?;
    let parsed: Value = toml::from_str(&raw).map_err(|source| PlanError::CorpusParse {
        path: args.corpus.clone(),
        source,
    })?;

    // Pull the top-level `[corpus]` table (or empty table) and the
    // `[[entry]]` array. The toml crate emits arrays-of-tables under
    // the key name; we use "entry" to match the build subcommand.
    let top = parsed.as_table().cloned().unwrap_or_default();
    let corpus_meta = top.get("corpus").cloned();
    let entries: Vec<Value> = top
        .get("entry")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Partition entries into shard buckets. BTreeMap so iteration is
    // ascending shard_id deterministically.
    let mut buckets: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    for (ordinal, entry) in entries.iter().enumerate() {
        let source_url = entry
            .get("source_url")
            .and_then(Value::as_str)
            .ok_or(PlanError::EntryMissingSourceUrl { ordinal })?;
        let shard = shard_id(source_url, args.shards);
        buckets.entry(shard).or_default().push(entry.clone());
    }

    fs::create_dir_all(&args.out).map_err(|source| PlanError::OutputDir {
        path: args.out.clone(),
        source,
    })?;

    // Emit one TOML file per non-empty shard, in ascending shard_id
    // order (BTreeMap iteration). Empty shards are skipped — `attestrum
    // build` is not invoked on missing input files, and skipping keeps
    // the on-disk layout tidy.
    let mut emitted: Vec<u32> = Vec::with_capacity(buckets.len());
    for (shard, rows) in &buckets {
        let mut table = toml::map::Map::new();
        if let Some(meta) = corpus_meta.clone() {
            table.insert("corpus".to_string(), meta);
        }
        table.insert("entry".to_string(), Value::Array(rows.clone()));
        let serialized = toml::to_string(&Value::Table(table))?;
        let path = args.out.join(format!("shard-{shard:04}.toml"));
        fs::write(&path, serialized).map_err(|source| PlanError::EmitShard {
            path: path.clone(),
            source,
        })?;
        emitted.push(*shard);
    }

    tracing::info!(
        corpus = %args.corpus.display(),
        shards_requested = args.shards,
        shards_emitted = emitted.len(),
        entries = entries.len(),
        "plan complete"
    );
    println!("attestrum plan: ok");
    println!("  shards_requested: {}", args.shards);
    println!("  shards_emitted:   {}", emitted.len());
    println!("  entries:          {}", entries.len());
    println!("  out:              {}", args.out.display());
    Ok(())
}

/// Deterministic shard assignment. `BLAKE3(source_url.as_bytes())`'s
/// first 8 bytes interpreted as little-endian u64, modulo `shards`.
/// Pure function; same `source_url` + same `shards` → same `shard_id`.
pub fn shard_id(source_url: &str, shards: u32) -> u32 {
    let hash = blake3::hash(source_url.as_bytes());
    let bytes = hash.as_bytes();
    let mut first8 = [0u8; 8];
    first8.copy_from_slice(&bytes[..8]);
    let n = u64::from_le_bytes(first8);
    (n % u64::from(shards)) as u32
}
