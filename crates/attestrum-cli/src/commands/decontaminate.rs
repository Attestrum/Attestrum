//! `attestrum decontaminate` — scan a corpus for leaked evaluation-benchmark
//! items and emit an unsigned contamination report.
//!
//! Read-only and network-free: it reads the corpus and benchmark files, runs
//! the deterministic `attestrum-decontaminate` scan (normalize + three signals
//! over the shared MinHash kernel), and writes `report.json` + `report.md`.
//! Modifies no manifest and emits no signed predicate.

use attestrum_decontaminate::detect::{scan, BenchItem, Benchmark};
use attestrum_decontaminate::ingest::{read_corpus, IngestError};
use attestrum_decontaminate::report;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug)]
pub struct Args {
    /// Corpus file(s) to scan.
    pub corpus: Vec<PathBuf>,
    /// Benchmark file(s) to scan against; each file is one benchmark.
    pub against: Vec<PathBuf>,
    /// JSON field / Parquet column holding the text.
    pub text_key: String,
    /// MinHash Jaccard threshold for the `near` signal.
    pub near_threshold: f64,
    /// Output directory for `report.json` + `report.md`.
    pub out: PathBuf,
}

#[derive(Debug, Error)]
pub enum DecontaminateCliError {
    #[error("--near-threshold must be in [0.0, 1.0], got {0}")]
    ThresholdOutOfRange(f64),

    #[error("two benchmark files resolve to the same name {0:?}; rename one so each benchmark is distinct")]
    DuplicateBenchmark(String),

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

pub fn run(args: Args) -> Result<(), DecontaminateCliError> {
    if !(0.0..=1.0).contains(&args.near_threshold) {
        return Err(DecontaminateCliError::ThresholdOutOfRange(
            args.near_threshold,
        ));
    }

    // Load benchmarks: one file → one benchmark, named by file stem.
    let mut benchmarks: Vec<Benchmark> = Vec::with_capacity(args.against.len());
    let mut bench_totals: BTreeMap<String, usize> = BTreeMap::new();
    for path in &args.against {
        let name = benchmark_name(path);
        if bench_totals.contains_key(&name) {
            return Err(DecontaminateCliError::DuplicateBenchmark(name));
        }
        let items_docs =
            read_corpus(path, &args.text_key).map_err(|source| DecontaminateCliError::Ingest {
                path: path.clone(),
                source,
            })?;
        let items: Vec<BenchItem> = items_docs
            .into_iter()
            .map(|d| BenchItem::new(d.id, &d.text))
            .collect();
        bench_totals.insert(name.clone(), items.len());
        benchmarks.push(Benchmark { name, items });
    }

    // Load corpus documents from all files (concatenated; doc ids carry the
    // file stem so they stay distinct across files).
    let mut docs = Vec::new();
    let mut corpus_files: Vec<String> = Vec::with_capacity(args.corpus.len());
    for path in &args.corpus {
        let mut these =
            read_corpus(path, &args.text_key).map_err(|source| DecontaminateCliError::Ingest {
                path: path.clone(),
                source,
            })?;
        corpus_files.push(path.display().to_string());
        docs.append(&mut these);
    }

    let (hits, stats) = scan(&docs, &benchmarks, args.near_threshold);
    let report = report::build(
        corpus_files,
        stats,
        &bench_totals,
        &hits,
        args.near_threshold,
        // No timestamp: keeps the report a pure function of its inputs.
        None,
    );

    let json = report.to_json().map_err(DecontaminateCliError::Serialize)?;
    let markdown = report.to_markdown();

    std::fs::create_dir_all(&args.out).map_err(|source| DecontaminateCliError::CreateOut {
        path: args.out.clone(),
        source,
    })?;
    let json_path = args.out.join("report.json");
    let md_path = args.out.join("report.md");
    std::fs::write(&json_path, json.as_bytes()).map_err(|source| DecontaminateCliError::Write {
        path: json_path.clone(),
        source,
    })?;
    std::fs::write(&md_path, markdown.as_bytes()).map_err(|source| {
        DecontaminateCliError::Write {
            path: md_path.clone(),
            source,
        }
    })?;

    let total_hits = hits.len();
    println!(
        "decontaminate: scanned {} document(s) against {} benchmark(s) → {} hit(s)",
        stats.docs,
        benchmarks.len(),
        total_hits
    );
    println!("  {}", json_path.display());
    println!("  {}", md_path.display());
    Ok(())
}

/// A benchmark's report name is its file stem (e.g. `mmlu.parquet` → `mmlu`).
fn benchmark_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("benchmark")
        .to_string()
}
