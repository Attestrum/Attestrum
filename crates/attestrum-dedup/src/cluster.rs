//! Near-duplicate clustering.
//!
//! Pipeline: normalize → MinHash signature → collapse exact-identical
//! signatures into representatives → LSH banding for candidate generation →
//! exact Jaccard verify → union-find into clusters.
//!
//! Collapsing exact-identical signatures first means a corpus that is mostly
//! verbatim copies (the degenerate case for an all-pairs join) reduces to one
//! representative per distinct signature *before* the quadratic step, so a
//! single huge identical group costs O(n), not O(n²). Distinct-but-similar
//! documents are then linked by LSH-banded candidates and a Jaccard check.

use attestrum_decontaminate::ingest::Doc;
use attestrum_text_minhash::{minhash, normalize_text};
use std::collections::{BTreeSet, HashMap};

/// MinHash permutation count of the PROTECTED kernel. If the kernel changes
/// this, every signature changes and the committed golden breaks — the intended
/// tripwire.
pub const MINHASH_PERMUTATIONS: usize = 128;

/// LSH band count for candidate generation.
pub const LSH_BANDS: usize = 16;
/// LSH rows per band. `LSH_BANDS * LSH_ROWS == MINHASH_PERMUTATIONS`.
pub const LSH_ROWS: usize = 8;

const _: () = assert!(LSH_BANDS * LSH_ROWS == MINHASH_PERMUTATIONS);

/// Default MinHash Jaccard threshold for the near-duplicate verify step.
pub const DEFAULT_NEAR_THRESHOLD: f64 = 0.80;

/// Maximum example clusters retained in the report (largest first).
pub const MAX_EXAMPLE_CLUSTERS: usize = 50;

/// One near-duplicate cluster: the ids of every document in it (sorted).
pub struct Cluster {
    pub document_ids: Vec<String>,
}

/// Outcome of a dedup run.
pub struct DedupResult {
    pub documents: usize,
    /// Documents that fall into some near-duplicate cluster.
    pub near_duplicate_documents: usize,
    /// Clusters (size ≥ 2 documents), sorted by size desc then first id asc.
    pub clusters: Vec<Cluster>,
}

impl DedupResult {
    /// Fraction of documents in some near-duplicate cluster (6-decimal rounded).
    pub fn near_duplicate_rate(&self) -> f64 {
        if self.documents == 0 {
            0.0
        } else {
            round6(self.near_duplicate_documents as f64 / self.documents as f64)
        }
    }
}

/// Distinct signature + the document indices that share it exactly.
struct Rep {
    signature: Vec<u64>,
    members: Vec<usize>,
}

/// Cluster `docs` by near-duplicate similarity at `near_threshold`.
pub fn dedup(docs: &[Doc], near_threshold: f64) -> DedupResult {
    let n = docs.len();

    // 1. Signature per document, then collapse exact-identical signatures into
    //    representatives (members carried in document order).
    let mut sig_to_rep: HashMap<Vec<u64>, usize> = HashMap::new();
    let mut reps: Vec<Rep> = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let signature = minhash::compute(&normalize_text(&doc.text));
        match sig_to_rep.get(&signature) {
            Some(&r) => reps[r].members.push(i),
            None => {
                sig_to_rep.insert(signature.clone(), reps.len());
                reps.push(Rep {
                    signature,
                    members: vec![i],
                });
            }
        }
    }

    // 2. LSH banding over representatives → candidate rep-pairs. Each rep sits
    //    in exactly LSH_BANDS buckets; bucket iteration order is irrelevant
    //    because pairs land in a sorted BTreeSet.
    let mut buckets: HashMap<(usize, Vec<u64>), Vec<usize>> = HashMap::new();
    for (r, rep) in reps.iter().enumerate() {
        for (band_idx, band) in rep.signature.chunks(LSH_ROWS).enumerate() {
            buckets
                .entry((band_idx, band.to_vec()))
                .or_default()
                .push(r);
        }
    }
    let mut candidates: BTreeSet<(usize, usize)> = BTreeSet::new();
    for members in buckets.values() {
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                let (i, j) = (members[a], members[b]);
                candidates.insert(if i < j { (i, j) } else { (j, i) });
            }
        }
    }

    // 3. Verify candidates by exact Jaccard; union-find the survivors.
    let mut uf = UnionFind::new(reps.len());
    for &(i, j) in &candidates {
        if jaccard_estimate(&reps[i].signature, &reps[j].signature) >= near_threshold {
            uf.union(i, j);
        }
    }

    // 4. Expand rep-components back to document clusters (size ≥ 2 documents).
    let mut components: HashMap<usize, Vec<usize>> = HashMap::new();
    for r in 0..reps.len() {
        components.entry(uf.find(r)).or_default().push(r);
    }
    let mut clusters: Vec<Cluster> = components
        .values()
        .filter_map(|rep_indices| {
            let mut ids: Vec<String> = rep_indices
                .iter()
                .flat_map(|&r| reps[r].members.iter().map(|&d| docs[d].id.clone()))
                .collect();
            if ids.len() < 2 {
                return None;
            }
            ids.sort();
            Some(Cluster { document_ids: ids })
        })
        .collect();
    clusters.sort_by(|a, b| {
        b.document_ids
            .len()
            .cmp(&a.document_ids.len())
            .then_with(|| a.document_ids[0].cmp(&b.document_ids[0]))
    });

    let near_duplicate_documents = clusters.iter().map(|c| c.document_ids.len()).sum();

    DedupResult {
        documents: n,
        near_duplicate_documents,
        clusters,
    }
}

/// Estimate Jaccard similarity as the fraction of matching signature
/// components. Two empty-input signatures (all `u64::MAX`) report 0.0, not 1.0 —
/// matching `attestrum-decontaminate`'s convention so the basis is identical.
fn jaccard_estimate(a: &[u64], b: &[u64]) -> f64 {
    debug_assert_eq!(a.len(), b.len(), "signatures must be equal length");
    let all_max = |s: &[u64]| s.iter().all(|&x| x == u64::MAX);
    if all_max(a) && all_max(b) {
        return 0.0;
    }
    let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    matches as f64 / a.len() as f64
}

/// Round to 6 decimals so report float formatting is stable.
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

/// Disjoint-set union with path halving + union by size.
struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (big, small) = if self.size[ra] >= self.size[rb] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent[small] = big;
        self.size[big] += self.size[small];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, text: &str) -> Doc {
        Doc {
            id: id.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn exact_duplicates_cluster_without_lsh_edges() {
        let docs = vec![
            doc(
                "a",
                "the quick brown fox jumps over the lazy dog again and again",
            ),
            doc(
                "b",
                "the quick brown fox jumps over the lazy dog again and again",
            ),
            doc(
                "c",
                "an entirely unrelated sentence about marine biology and tides",
            ),
        ];
        let r = dedup(&docs, 0.8);
        assert_eq!(r.clusters.len(), 1);
        assert_eq!(r.clusters[0].document_ids, vec!["a", "b"]);
        assert_eq!(r.near_duplicate_documents, 2);
        assert_eq!(r.near_duplicate_rate(), round6(2.0 / 3.0));
    }

    #[test]
    fn unrelated_documents_form_no_cluster() {
        let docs = vec![
            doc(
                "a",
                "alpha beta gamma delta epsilon zeta eta theta iota kappa",
            ),
            doc(
                "b",
                "one two three four five six seven eight nine ten eleven twelve",
            ),
        ];
        let r = dedup(&docs, 0.8);
        assert!(r.clusters.is_empty());
        assert_eq!(r.near_duplicate_documents, 0);
        assert_eq!(r.near_duplicate_rate(), 0.0);
    }

    #[test]
    fn empty_corpus_is_zero_rate() {
        let r = dedup(&[], 0.8);
        assert_eq!(r.documents, 0);
        assert_eq!(r.near_duplicate_rate(), 0.0);
    }
}
