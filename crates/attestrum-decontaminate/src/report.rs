//! Report assembly and serialization.
//!
//! The canonical `report.json` is produced through
//! [`attestrum_attest::deterministic_json`] — the workspace's single sanctioned
//! sort-then-serialize primitive (recursive object-key sort, compact output).
//! Reusing it keeps this report's determinism on the same tested basis as the
//! attestation emitters and `attestrum diff`, instead of a parallel hand-rolled
//! serializer. All keyed aggregates are `BTreeMap`s and the hit list is
//! pre-sorted, so the serialized bytes are a pure function of the scan inputs.

use crate::detect::{CorpusStats, Hit};
use crate::{DEFAULT_CONTAINMENT_THRESHOLD, EXACT_N, NEAR_N};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The PROTECTED kernel's permutation count
/// (`attestrum_text_minhash::minhash`). Recorded so a reader knows the MinHash
/// width behind the `near` signal; if the kernel ever changes it, every
/// signature changes and the committed golden breaks — the intended tripwire.
const MINHASH_PERMUTATIONS: usize = 128;

#[derive(Serialize)]
pub struct Report {
    pub tool: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub corpus: CorpusSection,
    pub parameters: Parameters,
    pub benchmarks: BTreeMap<String, BenchmarkSection>,
    pub hits: Vec<HitRecord>,
}

#[derive(Serialize)]
pub struct CorpusSection {
    pub files: Vec<String>,
    pub documents: usize,
    pub words: usize,
}

#[derive(Serialize)]
pub struct Parameters {
    pub exact_ngram: usize,
    pub near_ngram: usize,
    pub minhash_permutations: usize,
    pub near_threshold: f64,
    pub containment_threshold: f64,
}

#[derive(Serialize)]
pub struct BenchmarkSection {
    pub items_total: usize,
    pub items_flagged_exact: usize,
    pub items_flagged_near: usize,
    pub items_flagged_contained: usize,
    pub items_flagged_any: usize,
    pub contamination_rate: f64,
}

#[derive(Serialize, Clone)]
pub struct HitRecord {
    pub benchmark: String,
    pub item_id: String,
    pub doc_id: String,
    pub flags: Vec<String>,
    pub shared_exact_shingles: usize,
    pub jaccard_estimate: f64,
    pub containment: f64,
    pub item_snippet: String,
}

/// Build the report from scan output. `bench_totals` maps benchmark name to its
/// total item count, so clean benchmarks still appear in the report.
pub fn build(
    corpus_files: Vec<String>,
    stats: CorpusStats,
    bench_totals: &BTreeMap<String, usize>,
    hits: &[Hit],
    near_threshold: f64,
    timestamp: Option<String>,
) -> Report {
    let mut sections: BTreeMap<String, BenchmarkSection> = bench_totals
        .iter()
        .map(|(name, &total)| {
            (
                name.clone(),
                BenchmarkSection {
                    items_total: total,
                    items_flagged_exact: 0,
                    items_flagged_near: 0,
                    items_flagged_contained: 0,
                    items_flagged_any: 0,
                    contamination_rate: 0.0,
                },
            )
        })
        .collect();

    // Distinct flagged items per benchmark per category.
    let mut exact_items: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut near_items: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut contained_items: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut any_items: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for h in hits {
        if h.exact {
            exact_items
                .entry(&h.benchmark)
                .or_default()
                .insert(&h.item_id);
        }
        if h.near {
            near_items
                .entry(&h.benchmark)
                .or_default()
                .insert(&h.item_id);
        }
        if h.contained {
            contained_items
                .entry(&h.benchmark)
                .or_default()
                .insert(&h.item_id);
        }
        any_items
            .entry(&h.benchmark)
            .or_default()
            .insert(&h.item_id);
    }
    for (name, section) in sections.iter_mut() {
        let count =
            |m: &BTreeMap<&str, BTreeSet<&str>>| m.get(name.as_str()).map(|s| s.len()).unwrap_or(0);
        section.items_flagged_exact = count(&exact_items);
        section.items_flagged_near = count(&near_items);
        section.items_flagged_contained = count(&contained_items);
        section.items_flagged_any = count(&any_items);
        section.contamination_rate = if section.items_total == 0 {
            0.0
        } else {
            let rate = section.items_flagged_any as f64 / section.items_total as f64;
            (rate * 1_000_000.0).round() / 1_000_000.0
        };
    }

    let hit_records: Vec<HitRecord> = hits
        .iter()
        .map(|h| {
            let mut flags = Vec::new();
            if h.exact {
                flags.push("exact".to_string());
            }
            if h.near {
                flags.push("near".to_string());
            }
            if h.contained {
                flags.push("contained".to_string());
            }
            HitRecord {
                benchmark: h.benchmark.clone(),
                item_id: h.item_id.clone(),
                doc_id: h.doc_id.clone(),
                flags,
                shared_exact_shingles: h.shared_exact_shingles,
                jaccard_estimate: h.jaccard_estimate,
                containment: h.containment,
                item_snippet: h.item_snippet.clone(),
            }
        })
        .collect();

    Report {
        tool: "attestrum-decontaminate".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
        corpus: CorpusSection {
            files: corpus_files,
            documents: stats.docs,
            words: stats.words,
        },
        parameters: Parameters {
            exact_ngram: EXACT_N,
            near_ngram: NEAR_N,
            minhash_permutations: MINHASH_PERMUTATIONS,
            near_threshold,
            containment_threshold: DEFAULT_CONTAINMENT_THRESHOLD,
        },
        benchmarks: sections,
        hits: hit_records,
    }
}

impl Report {
    /// Serialize to canonical JSON (recursive key sort, compact) with a single
    /// trailing newline. Bytes are a pure function of the scan inputs.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut s = attestrum_attest::deterministic_json(self)?;
        s.push('\n');
        Ok(s)
    }

    /// Render the human-readable Markdown summary.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!(
            "# attestrum decontaminate report (v{})\n\n",
            self.version
        ));
        if let Some(ts) = &self.timestamp {
            md.push_str(&format!("- generated: {ts}\n"));
        }
        md.push_str(&format!(
            "- corpus: {} ({} documents, {} words)\n",
            self.corpus.files.join(", "),
            self.corpus.documents,
            self.corpus.words
        ));
        md.push_str(&format!(
            "- detection: {}-gram exact · minhash({}) jaccard ≥ {} · containment ≥ {}\n\n",
            self.parameters.exact_ngram,
            self.parameters.minhash_permutations,
            self.parameters.near_threshold,
            self.parameters.containment_threshold
        ));

        md.push_str("## Per-benchmark summary\n\n");
        md.push_str("| benchmark | items | exact | near | contained | flagged | rate |\n");
        md.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
        for (name, s) in &self.benchmarks {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {:.3}% |\n",
                name,
                s.items_total,
                s.items_flagged_exact,
                s.items_flagged_near,
                s.items_flagged_contained,
                s.items_flagged_any,
                s.contamination_rate * 100.0
            ));
        }
        md.push('\n');

        if self.hits.is_empty() {
            md.push_str("## Hits\n\nNo contamination detected.\n");
        } else {
            md.push_str(&format!("## Top hits ({} total)\n\n", self.hits.len()));
            let mut worst: Vec<&HitRecord> = self.hits.iter().collect();
            worst.sort_by(|a, b| {
                b.containment
                    .partial_cmp(&a.containment)
                    .expect("containment is never NaN")
                    .then(
                        b.jaccard_estimate
                            .partial_cmp(&a.jaccard_estimate)
                            .expect("jaccard is never NaN"),
                    )
                    .then(a.doc_id.cmp(&b.doc_id))
            });
            for h in worst.iter().take(10) {
                md.push_str(&format!(
                    "- **{}/{}** in doc `{}` — [{}] shared-13grams={} jaccard={:.2} containment={:.2}\n  > {}\n",
                    h.benchmark,
                    h.item_id,
                    h.doc_id,
                    h.flags.join(","),
                    h.shared_exact_shingles,
                    h.jaccard_estimate,
                    h.containment,
                    h.item_snippet
                ));
            }
        }
        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::CorpusStats;

    fn totals() -> BTreeMap<String, usize> {
        [("gsm8k".to_string(), 3usize)].into_iter().collect()
    }

    #[test]
    fn clean_corpus_renders_no_contamination() {
        let report = build(
            vec!["corpus.jsonl".into()],
            CorpusStats {
                docs: 5,
                words: 100,
            },
            &totals(),
            &[],
            0.8,
            None,
        );
        let md = report.to_markdown();
        assert!(md.contains("No contamination detected."));
        assert!(!md.contains("- generated:"));
        // Clean benchmark still appears with a zero rate.
        assert!(md.contains("| gsm8k | 3 | 0 | 0 | 0 | 0 | 0.000% |"));
    }

    #[test]
    fn timestamp_is_rendered_when_supplied() {
        let report = build(
            vec!["corpus.jsonl".into()],
            CorpusStats { docs: 1, words: 1 },
            &totals(),
            &[],
            0.8,
            Some("2026-06-12".to_string()),
        );
        assert!(report.to_markdown().contains("- generated: 2026-06-12"));
    }

    #[test]
    fn json_is_canonical_and_newline_terminated() {
        let report = build(
            vec!["corpus.jsonl".into()],
            CorpusStats { docs: 1, words: 1 },
            &totals(),
            &[],
            0.8,
            None,
        );
        let json = report.to_json().expect("serialize");
        assert!(json.ends_with("}\n"));
        // deterministic_json sorts keys: "benchmarks" precedes "corpus".
        let b = json.find("\"benchmarks\"").expect("benchmarks key");
        let c = json.find("\"corpus\"").expect("corpus key");
        assert!(b < c, "object keys must be recursively sorted");
    }
}
