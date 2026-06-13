//! `attestrum dedup` — emit an unsigned intra-corpus near-duplicate report.
//!
//! Read-only and network-free: it ingests the corpus file(s), recomputes
//! MinHash signatures via the shared PROTECTED kernel, clusters near-duplicates
//! (LSH banding + Jaccard verify + union-find), and writes `report.json` +
//! `report.md`. Modifies no manifest and emits no signed predicate.

use attestrum_decontaminate::ingest::{read_corpus, IngestError};
use attestrum_dedup::{cluster::dedup, report};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug)]
pub struct Args {
    pub corpus: Vec<PathBuf>,
    pub text_key: String,
    pub near_threshold: f64,
    pub out: PathBuf,
}

#[derive(Debug, Error)]
pub enum DedupCliError {
    #[error("--near-threshold must be in [0.0, 1.0], got {0}")]
    ThresholdOutOfRange(f64),

    #[error("reading {path}")]
    Ingest {
        path: PathBuf,
        #[source]
        source: IngestError,
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

pub fn run(args: Args) -> Result<(), DedupCliError> {
    if !(0.0..=1.0).contains(&args.near_threshold) {
        return Err(DedupCliError::ThresholdOutOfRange(args.near_threshold));
    }

    let mut docs = Vec::new();
    let mut corpus_files: Vec<String> = Vec::with_capacity(args.corpus.len());
    for path in &args.corpus {
        let mut these =
            read_corpus(path, &args.text_key).map_err(|source| DedupCliError::Ingest {
                path: path.clone(),
                source,
            })?;
        corpus_files.push(path.display().to_string());
        docs.append(&mut these);
    }

    let result = dedup(&docs, args.near_threshold);
    let report = report::build(corpus_files, &result, args.near_threshold, None);
    let json = report.to_json().map_err(DedupCliError::Serialize)?;
    let markdown = report.to_markdown();

    std::fs::create_dir_all(&args.out).map_err(|source| DedupCliError::CreateOut {
        path: args.out.clone(),
        source,
    })?;
    let json_path = args.out.join("report.json");
    let md_path = args.out.join("report.md");
    std::fs::write(&json_path, json.as_bytes()).map_err(|source| DedupCliError::Write {
        path: json_path.clone(),
        source,
    })?;
    std::fs::write(&md_path, markdown.as_bytes()).map_err(|source| DedupCliError::Write {
        path: md_path.clone(),
        source,
    })?;

    println!(
        "dedup: {} document(s) → {} near-duplicate(s) in {} cluster(s) ({:.2}%)",
        result.documents,
        result.near_duplicate_documents,
        result.clusters.len(),
        result.near_duplicate_rate() * 100.0
    );
    println!("  {}", json_path.display());
    println!("  {}", md_path.display());
    Ok(())
}
