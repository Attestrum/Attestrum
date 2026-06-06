//! Build the fuzzy-lookup sidecar indexes from a sealed corpus.
//!
//! Reads the manifest once, opens the CAS, fingerprints each leaf once, and
//! routes signatures into the per-kind sub-indexes, then atomically writes each
//! sidecar under `<cas_root>/index/<kind>/v1.idx`. The index is discovery-grade
//! and unsigned — the signed inclusion proof stays in `attestrum-prove`.
//!
//! This commit lands the **text MinHash** sub-index; image perceptual and ISCC
//! are added in following commits behind the same one-pass loop.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use attestrum_cas::CasStore;
use attestrum_core::Modality;
use attestrum_fingerprint::{fingerprint_text, FingerprintOpts};
use attestrum_manifest::{read_manifest, ManifestEntry};
use attestrum_merkle::MerkleTree;

use crate::error::IndexError;
use crate::format::{FuzzyIndex, SigEntry, SubIndexKind};
use crate::query::{band_minhash, MINHASH_BANDS, MINHASH_PERMS, MINHASH_ROWS};

/// Per-kind build summary (leaves indexed + distinct buckets emitted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubReport {
    /// Number of leaves indexed for this kind.
    pub leaves: usize,
    /// Number of distinct `(band_id, band_hash)` buckets.
    pub buckets: usize,
}

/// Summary of a `build_all` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildReport {
    /// The text MinHash sub-index.
    pub minhash: SubReport,
}

/// The on-disk path for a sub-index: `<cas_root>/index/<kind>/v1.idx`.
pub fn sidecar_path(cas_root: &Path, kind: SubIndexKind) -> PathBuf {
    cas_root.join("index").join(kind.subdir()).join("v1.idx")
}

/// The corpus binding root the querier checks against: the RFC 6962 Merkle root
/// over `document_id` in manifest row order — byte-identical to the root
/// `attestrum-prove` recomputes (`MerkleTree::new(document_ids).root()`).
fn binding_root(entries: &[ManifestEntry]) -> [u8; 32] {
    MerkleTree::new(entries.iter().map(|e| e.document_id).collect()).root()
}

fn read_leaf(cas: &CasStore, digest: &[u8; 32]) -> Result<Vec<u8>, IndexError> {
    let mut f = cas.open(digest)?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Build all fuzzy sidecar indexes for the sealed corpus at `manifest_path` /
/// `cas_root`, writing them under `<cas_root>/index/<kind>/v1.idx`.
///
/// Leaves that fail to fingerprint (e.g. non-UTF-8 bytes on the text path) are
/// skipped rather than aborting the build — they simply are not indexed, and
/// the exhaustive fallback in `prove` still covers them.
pub fn build_all(
    manifest_path: &Path,
    cas_root: &Path,
    source_date_epoch: i64,
) -> Result<BuildReport, IndexError> {
    let entries = read_manifest(manifest_path).map_err(|e| IndexError::Manifest(e.to_string()))?;
    let root = binding_root(&entries);
    let cas = CasStore::new(cas_root)?;
    let fopts = FingerprintOpts { source_date_epoch };

    // Text MinHash accumulators.
    let mut mh_sigs: Vec<SigEntry> = Vec::new();
    let mut mh_buckets: BTreeMap<(u16, u64), Vec<u64>> = BTreeMap::new();

    for (row, entry) in entries.iter().enumerate() {
        let row = row as u64;
        if matches!(entry.modality, Modality::Text) {
            let bytes = read_leaf(&cas, &entry.document_id)?;
            let Ok(bundle) = fingerprint_text(&bytes, &fopts) else {
                continue;
            };
            if let Some(text) = bundle.text {
                for key in band_minhash(&text.minhash) {
                    mh_buckets.entry(key).or_default().push(row);
                }
                mh_sigs.push(SigEntry {
                    row,
                    sig: text.minhash,
                });
            }
        }
    }

    let mh_bucket_count = mh_buckets.len();
    let mh = FuzzyIndex::from_parts(
        SubIndexKind::Minhash,
        root,
        MINHASH_PERMS as u16,
        MINHASH_BANDS,
        MINHASH_ROWS as u16,
        mh_sigs,
        mh_buckets,
    )?;
    let report = BuildReport {
        minhash: SubReport {
            leaves: mh.leaf_count(),
            buckets: mh_bucket_count,
        },
    };
    mh.write_to_path(&sidecar_path(cas_root, SubIndexKind::Minhash))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use attestrum_manifest::{
        assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest,
        ManifestSignals,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_root(name: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "attestrum-index-build-{}-{name}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test root");
        root
    }

    /// Seal a tiny corpus into `<root>/cas-root/` + a manifest, mirroring the
    /// `build_corpus_with_cas` helper in `attestrum-prove`'s fuzzy tests.
    fn seal(root: &Path, items: &[(&[u8], Modality)]) -> (PathBuf, PathBuf) {
        let cas_root = root.join("cas-root");
        let cas = CasStore::new(&cas_root).expect("create cas");
        let mut entries: Vec<ManifestEntry> = items
            .iter()
            .map(|(bytes, modality)| {
                let h = attestrum_cas::stream_hash(*bytes).expect("hash bytes");
                cas.put(&h.blake3, bytes).expect("cas put");
                ManifestEntry {
                    document_id: h.blake3,
                    sha256: h.sha256,
                    size_bytes: h.size_bytes,
                    modality: *modality,
                    mime_type: None,
                    source_url: None,
                    source_type: None,
                    source_dataset_id: None,
                    registered_domain: None,
                    license_spdx: None,
                    language: None,
                    fetched_at: None,
                    signals: ManifestSignals::default(),
                    included: true,
                    exclusion_reason: None,
                    chunk_refs: None,
                    input_ordinal: 0,
                    occurrence_index: 0,
                }
            })
            .collect();
        assign_input_ordinals(&mut entries);
        sort_entries(&mut entries);
        assign_occurrence_indices(&mut entries);
        let manifest_path = root.join("manifest.parquet");
        write_manifest(&manifest_path, &entries).expect("write manifest");
        (manifest_path, cas_root)
    }

    const SDE: i64 = 1_700_000_000;

    // A long-ish base passage so a one-word edit keeps Jaccard well above 0.85.
    const BASE: &[u8] = b"the quick brown fox jumps over the lazy dog while the \
        industrious bee gathers nectar from the blooming wildflowers under a warm \
        afternoon sun and a gentle breeze drifts across the quiet meadow carrying \
        the soft scent of clover and fresh cut grass toward the distant treeline";
    const OTHER: &[u8] = b"financial regulators published new guidance on capital \
        adequacy ratios for systemically important institutions facing liquidity \
        stress during volatile interest rate environments and cross border exposure";

    fn minhash_of(bytes: &[u8]) -> Vec<u64> {
        let b = fingerprint_text(
            bytes,
            &FingerprintOpts {
                source_date_epoch: SDE,
            },
        )
        .expect("fingerprint");
        b.text.expect("text fp").minhash
    }

    fn row_of(manifest_path: &Path, bytes: &[u8]) -> u64 {
        let want = attestrum_cas::stream_hash(bytes).expect("hash").blake3;
        let entries = read_manifest(manifest_path).expect("read manifest");
        entries
            .iter()
            .position(|e| e.document_id == want)
            .expect("leaf present") as u64
    }

    #[test]
    fn builds_minhash_sidecar_for_text_corpus() {
        let root = fresh_root("build");
        let (manifest, cas_root) = seal(&root, &[(BASE, Modality::Text), (OTHER, Modality::Text)]);
        let report = build_all(&manifest, &cas_root, SDE).expect("build");
        assert_eq!(report.minhash.leaves, 2);
        let path = sidecar_path(&cas_root, SubIndexKind::Minhash);
        assert!(path.exists());
        let idx = FuzzyIndex::from_bytes(&std::fs::read(&path).unwrap()).expect("load sidecar");
        assert_eq!(idx.kind(), SubIndexKind::Minhash);
        assert_eq!(idx.leaf_count(), 2);
        // binding root matches what prove would recompute
        let entries = read_manifest(&manifest).unwrap();
        assert_eq!(idx.binding_root(), binding_root(&entries));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sidecar_rebuild_is_byte_identical() {
        let root = fresh_root("determinism");
        let (manifest, cas_root) = seal(&root, &[(BASE, Modality::Text), (OTHER, Modality::Text)]);
        build_all(&manifest, &cas_root, SDE).expect("build 1");
        let bytes1 = std::fs::read(sidecar_path(&cas_root, SubIndexKind::Minhash)).unwrap();
        build_all(&manifest, &cas_root, SDE).expect("build 2");
        let bytes2 = std::fs::read(sidecar_path(&cas_root, SubIndexKind::Minhash)).unwrap();
        assert_eq!(bytes1, bytes2, "rebuild must be byte-identical");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn self_query_is_a_candidate() {
        let root = fresh_root("self");
        let (manifest, cas_root) = seal(&root, &[(BASE, Modality::Text), (OTHER, Modality::Text)]);
        build_all(&manifest, &cas_root, SDE).expect("build");
        let idx = FuzzyIndex::from_bytes(
            &std::fs::read(sidecar_path(&cas_root, SubIndexKind::Minhash)).unwrap(),
        )
        .unwrap();
        let cands = idx.candidates(&band_minhash(&minhash_of(BASE)));
        assert!(
            cands.contains(&row_of(&manifest, BASE)),
            "exact self-query must surface its own leaf as a candidate"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn near_duplicate_is_a_candidate() {
        let root = fresh_root("neardup");
        let (manifest, cas_root) = seal(&root, &[(BASE, Modality::Text), (OTHER, Modality::Text)]);
        build_all(&manifest, &cas_root, SDE).expect("build");
        let idx = FuzzyIndex::from_bytes(
            &std::fs::read(sidecar_path(&cas_root, SubIndexKind::Minhash)).unwrap(),
        )
        .unwrap();
        // one-word edit of BASE → high Jaccard, must still bucket-collide
        let query = b"the quick brown fox leaps over the lazy dog while the \
            industrious bee gathers nectar from the blooming wildflowers under a warm \
            afternoon sun and a gentle breeze drifts across the quiet meadow carrying \
            the soft scent of clover and fresh cut grass toward the distant treeline";
        let cands = idx.candidates(&band_minhash(&minhash_of(query)));
        assert!(
            cands.contains(&row_of(&manifest, BASE)),
            "near-duplicate query must surface the base leaf as a candidate"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
