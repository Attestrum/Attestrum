//! Report assembly and serialization.
//!
//! The canonical `report.json` is produced through
//! [`attestrum_attest::deterministic_json`] — the workspace's single sanctioned
//! sort-then-serialize primitive. All lists are pre-sorted and all percentages
//! 6-decimal-rounded, so the bytes are a pure function of the corpus inputs.

use crate::cluster::{
    Cluster, DedupResult, LSH_BANDS, LSH_ROWS, MAX_EXAMPLE_CLUSTERS, MINHASH_PERMUTATIONS,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Serialize)]
pub struct Report {
    pub tool: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub corpus: CorpusSection,
    pub parameters: Parameters,
    pub summary: Summary,
    pub cluster_size_histogram: Vec<HistogramBin>,
    pub example_clusters: Vec<ExampleCluster>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_clusters_omitted: Option<usize>,
}

#[derive(Serialize)]
pub struct CorpusSection {
    pub files: Vec<String>,
    pub documents: usize,
}

#[derive(Serialize)]
pub struct Parameters {
    pub minhash_permutations: usize,
    pub lsh_bands: usize,
    pub lsh_rows: usize,
    pub near_threshold: f64,
}

#[derive(Serialize)]
pub struct Summary {
    pub documents: usize,
    pub near_duplicate_documents: usize,
    /// Fraction of documents in some near-duplicate cluster (6-decimal rounded).
    pub near_duplicate_rate: f64,
    pub clusters: usize,
    pub largest_cluster: usize,
}

#[derive(Serialize)]
pub struct HistogramBin {
    pub size: usize,
    pub clusters: usize,
}

#[derive(Serialize)]
pub struct ExampleCluster {
    pub size: usize,
    pub document_ids: Vec<String>,
}

/// Build the report from a dedup result. `corpus_files` is the caller-facing
/// list of inputs; pass fixed labels in tests so the golden is machine-independent.
pub fn build(
    corpus_files: Vec<String>,
    result: &DedupResult,
    near_threshold: f64,
    timestamp: Option<String>,
) -> Report {
    let largest_cluster = result
        .clusters
        .first()
        .map(|c| c.document_ids.len())
        .unwrap_or(0);

    // Cluster-size histogram, ascending by size.
    let mut by_size: BTreeMap<usize, usize> = BTreeMap::new();
    for c in &result.clusters {
        *by_size.entry(c.document_ids.len()).or_default() += 1;
    }
    let cluster_size_histogram = by_size
        .into_iter()
        .map(|(size, clusters)| HistogramBin { size, clusters })
        .collect();

    // Bounded example clusters (already sorted size-desc, then first-id-asc).
    let example_clusters: Vec<ExampleCluster> = result
        .clusters
        .iter()
        .take(MAX_EXAMPLE_CLUSTERS)
        .map(|c: &Cluster| ExampleCluster {
            size: c.document_ids.len(),
            document_ids: c.document_ids.clone(),
        })
        .collect();
    let example_clusters_omitted = result
        .clusters
        .len()
        .checked_sub(MAX_EXAMPLE_CLUSTERS)
        .filter(|&omitted| omitted > 0);

    Report {
        tool: "attestrum-dedup".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
        corpus: CorpusSection {
            files: corpus_files,
            documents: result.documents,
        },
        parameters: Parameters {
            minhash_permutations: MINHASH_PERMUTATIONS,
            lsh_bands: LSH_BANDS,
            lsh_rows: LSH_ROWS,
            near_threshold,
        },
        summary: Summary {
            documents: result.documents,
            near_duplicate_documents: result.near_duplicate_documents,
            near_duplicate_rate: result.near_duplicate_rate(),
            clusters: result.clusters.len(),
            largest_cluster,
        },
        cluster_size_histogram,
        example_clusters,
        example_clusters_omitted,
    }
}

impl Report {
    /// Serialize to canonical JSON (recursive key sort, compact) with a single
    /// trailing newline. Bytes are a pure function of the corpus inputs.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut s = attestrum_attest::deterministic_json(self)?;
        s.push('\n');
        Ok(s)
    }

    /// Render the human-readable Markdown summary.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        let _ = write!(
            md,
            "# attestrum dedup — near-duplicate report (v{})\n\n",
            self.version
        );
        if let Some(ts) = &self.timestamp {
            let _ = writeln!(md, "- generated: {ts}");
        }
        let _ = writeln!(md, "- corpus: {}", self.corpus.files.join(", "));
        let _ = writeln!(
            md,
            "- detection: minhash({}) jaccard ≥ {} via LSH {}×{} banding\n",
            self.parameters.minhash_permutations,
            self.parameters.near_threshold,
            self.parameters.lsh_bands,
            self.parameters.lsh_rows
        );

        let s = &self.summary;
        let _ = writeln!(
            md,
            "**{} of {} documents ({:.2}%) are near-duplicates**, in {} cluster(s); largest cluster {} docs.\n",
            s.near_duplicate_documents,
            s.documents,
            s.near_duplicate_rate * 100.0,
            s.clusters,
            s.largest_cluster
        );

        if !self.cluster_size_histogram.is_empty() {
            md.push_str(
                "## Cluster-size histogram\n\n| cluster size (docs) | count |\n|---:|---:|\n",
            );
            for bin in &self.cluster_size_histogram {
                let _ = writeln!(md, "| {} | {} |", bin.size, bin.clusters);
            }
            md.push('\n');
        }

        if self.example_clusters.is_empty() {
            md.push_str("## Clusters\n\nNo near-duplicates detected.\n");
        } else {
            let _ = write!(
                md,
                "## Example clusters ({} shown",
                self.example_clusters.len()
            );
            if let Some(omitted) = self.example_clusters_omitted {
                let _ = write!(md, ", {omitted} more omitted");
            }
            md.push_str(")\n\n");
            for (i, c) in self.example_clusters.iter().enumerate() {
                let _ = writeln!(
                    md,
                    "{}. ({} docs) {}",
                    i + 1,
                    c.size,
                    c.document_ids.join(", ")
                );
            }
        }
        md
    }
}
