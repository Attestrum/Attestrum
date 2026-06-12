//! The detection engine: exact 13-gram collision, MinHash near-duplicate, and
//! containment scoring of benchmark items against corpus documents.
//!
//! Normalization and the MinHash signature come from the PROTECTED kernel
//! (`attestrum_text_minhash`), so the near-duplicate basis is byte-identical to
//! `attestrum index` / `attestrum prove`. The exact and containment signals use
//! BLAKE3 shingle *sets* from [`crate::shingle`].
//!
//! Parallelism note: documents are scanned with rayon, but every hit is
//! collected and sorted before reporting — thread scheduling can never change
//! the output bytes.

use crate::ingest::Doc;
use crate::shingle::shingle_hashes;
use crate::{DEFAULT_CONTAINMENT_THRESHOLD, EXACT_N, NEAR_N, SNIPPET_CHARS};
use attestrum_text_minhash::{minhash, normalize_text};
use rayon::prelude::*;
use std::collections::HashMap;

/// A benchmark loaded for scanning.
pub struct Benchmark {
    pub name: String,
    pub items: Vec<BenchItem>,
}

/// One benchmark item, preprocessed for all three signals.
pub struct BenchItem {
    pub id: String,
    pub snippet: String,
    exact_shingles: Vec<u64>,
    near_shingles: Vec<u64>,
    signature: Vec<u64>,
}

impl BenchItem {
    pub fn new(id: String, text: &str) -> Self {
        let normalized = normalize_text(text);
        let exact_shingles = shingle_hashes(&normalized, EXACT_N);
        let near_shingles = shingle_hashes(&normalized, NEAR_N);
        let signature = minhash::compute(&normalized);
        let snippet = truncate(&normalized, SNIPPET_CHARS);
        BenchItem {
            id,
            snippet,
            exact_shingles,
            near_shingles,
            signature,
        }
    }
}

/// One flagged (document, benchmark item) pair.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub benchmark: String,
    pub item_id: String,
    pub item_snippet: String,
    pub doc_id: String,
    pub shared_exact_shingles: usize,
    pub jaccard_estimate: f64,
    pub containment: f64,
    pub exact: bool,
    pub near: bool,
    pub contained: bool,
}

/// Aggregate statistics about the scanned corpus.
#[derive(Clone, Copy, Debug, Default)]
pub struct CorpusStats {
    pub docs: usize,
    pub words: usize,
}

#[derive(Clone, Copy)]
struct ItemRef {
    bench: u32,
    item: u32,
}

/// Scan `docs` against `benchmarks`, returning all hits in deterministic order
/// plus corpus stats. A (doc, item) pair becomes a hit when ANY signal fires:
/// shared 13-gram (exact), MinHash Jaccard ≥ `near_threshold` (near), or
/// containment ≥ [`DEFAULT_CONTAINMENT_THRESHOLD`] (contained).
pub fn scan(
    docs: &[Doc],
    benchmarks: &[Benchmark],
    near_threshold: f64,
) -> (Vec<Hit>, CorpusStats) {
    // Inverted indexes over all benchmark items.
    let mut exact_index: HashMap<u64, Vec<ItemRef>> = HashMap::new();
    let mut near_index: HashMap<u64, Vec<ItemRef>> = HashMap::new();
    for (bi, bench) in benchmarks.iter().enumerate() {
        for (ii, item) in bench.items.iter().enumerate() {
            let r = ItemRef {
                bench: bi as u32,
                item: ii as u32,
            };
            for &h in &item.exact_shingles {
                exact_index.entry(h).or_default().push(r);
            }
            for &h in &item.near_shingles {
                near_index.entry(h).or_default().push(r);
            }
        }
    }

    let results: Vec<(Vec<Hit>, usize)> = docs
        .par_iter()
        .map(|doc| {
            let normalized = normalize_text(&doc.text);
            let words = normalized.split(' ').filter(|w| !w.is_empty()).count();
            let exact = shingle_hashes(&normalized, EXACT_N);
            let near = shingle_hashes(&normalized, NEAR_N);
            let sig = minhash::compute(&normalized);

            // Shared shingle counts per candidate item. Both shingle vectors
            // are deduplicated sets, so these counts are exact intersection
            // sizes.
            let mut exact_shared: HashMap<(u32, u32), usize> = HashMap::new();
            for h in &exact {
                if let Some(refs) = exact_index.get(h) {
                    for r in refs {
                        *exact_shared.entry((r.bench, r.item)).or_default() += 1;
                    }
                }
            }
            let mut near_shared: HashMap<(u32, u32), usize> = HashMap::new();
            for h in &near {
                if let Some(refs) = near_index.get(h) {
                    for r in refs {
                        *near_shared.entry((r.bench, r.item)).or_default() += 1;
                    }
                }
            }

            // Union of candidates surfaced by either signal.
            let mut candidates: Vec<(u32, u32)> = exact_shared
                .keys()
                .chain(near_shared.keys())
                .copied()
                .collect();
            candidates.sort_unstable();
            candidates.dedup();

            let mut hits = Vec::new();
            for (bi, ii) in candidates {
                let bench = &benchmarks[bi as usize];
                let item = &bench.items[ii as usize];
                let shared_exact = exact_shared.get(&(bi, ii)).copied().unwrap_or(0);
                let shared_near = near_shared.get(&(bi, ii)).copied().unwrap_or(0);
                let containment = if item.near_shingles.is_empty() {
                    0.0
                } else {
                    shared_near as f64 / item.near_shingles.len() as f64
                };
                let jaccard = if shared_near > 0 {
                    jaccard_estimate(&sig, &item.signature)
                } else {
                    0.0
                };
                let exact_flag = shared_exact > 0;
                let near_flag = jaccard >= near_threshold;
                let contained_flag = containment >= DEFAULT_CONTAINMENT_THRESHOLD;
                if exact_flag || near_flag || contained_flag {
                    hits.push(Hit {
                        benchmark: bench.name.clone(),
                        item_id: item.id.clone(),
                        item_snippet: item.snippet.clone(),
                        doc_id: doc.id.clone(),
                        shared_exact_shingles: shared_exact,
                        jaccard_estimate: round6(jaccard),
                        containment: round6(containment),
                        exact: exact_flag,
                        near: near_flag,
                        contained: contained_flag,
                    });
                }
            }
            (hits, words)
        })
        .collect();

    let mut hits: Vec<Hit> = Vec::new();
    let mut stats = CorpusStats {
        docs: docs.len(),
        words: 0,
    };
    for (h, w) in results {
        hits.extend(h);
        stats.words += w;
    }
    // Deterministic total order: benchmark, item, then doc.
    hits.sort_by(|a, b| {
        (&a.benchmark, &a.item_id, &a.doc_id).cmp(&(&b.benchmark, &b.item_id, &b.doc_id))
    });
    (hits, stats)
}

/// Estimate Jaccard similarity as the fraction of matching signature
/// components. Two empty-input signatures (all `u64::MAX`) report 0.0, not 1.0.
fn jaccard_estimate(a: &[u64], b: &[u64]) -> f64 {
    debug_assert_eq!(a.len(), b.len(), "signatures must be equal length");
    let all_max = |s: &[u64]| s.iter().all(|&x| x == u64::MAX);
    if all_max(a) && all_max(b) {
        return 0.0;
    }
    let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    matches as f64 / a.len() as f64
}

/// Round to 6 decimal places so float formatting in the report is stable and
/// readable. The arithmetic is integer-exact (set sizes, signature-component
/// matches); rounding only trims the division result.
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, text: &str) -> Doc {
        Doc {
            id: id.into(),
            text: text.into(),
        }
    }

    fn bench(name: &str, items: &[(&str, &str)]) -> Benchmark {
        Benchmark {
            name: name.into(),
            items: items
                .iter()
                .map(|(id, t)| BenchItem::new(id.to_string(), t))
                .collect(),
        }
    }

    const ITEM: &str = "natalia sold clips to 48 of her friends in april and then she sold half as many clips in may how many clips did natalia sell altogether in april and may";

    #[test]
    fn verbatim_copy_fires_all_three() {
        let b = bench("fixture", &[("q1", ITEM)]);
        let (hits, _) = scan(&[doc("d1", ITEM)], &[b], 0.8);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].exact && hits[0].near && hits[0].contained);
    }

    #[test]
    fn embedded_item_fires_exact_and_contained_not_near() {
        let filler: String = (0..400).map(|i| format!("filler{i} ")).collect();
        let long_doc = format!("{filler} {ITEM} {filler}");
        let b = bench("fixture", &[("q1", ITEM)]);
        let (hits, _) = scan(&[doc("d1", &long_doc)], &[b], 0.8);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].exact, "verbatim embedding shares 13-grams");
        assert!(hits[0].contained, "containment should catch embedding");
        assert!(!hits[0].near, "jaccard is diluted by the long doc");
    }

    #[test]
    fn unrelated_doc_is_clean() {
        let b = bench("fixture", &[("q1", ITEM)]);
        let clean = "the weather in lisbon was mild and the trams ran on time through the old town all afternoon";
        let (hits, stats) = scan(&[doc("d1", clean)], &[b], 0.8);
        assert!(hits.is_empty());
        assert_eq!(stats.docs, 1);
    }

    #[test]
    fn hit_order_is_deterministic() {
        let b = bench("fixture", &[("q1", ITEM), ("q2", ITEM)]);
        let docs = vec![doc("d2", ITEM), doc("d1", ITEM)];
        let (h1, _) = scan(&docs, &[b], 0.8);
        let keys: Vec<_> = h1
            .iter()
            .map(|h| (h.item_id.clone(), h.doc_id.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn empty_signatures_do_not_match() {
        let e = vec![u64::MAX; 128];
        assert_eq!(jaccard_estimate(&e, &e), 0.0);
    }
}
