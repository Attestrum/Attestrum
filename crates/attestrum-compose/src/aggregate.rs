//! Manifest walk + composition aggregation.
//!
//! [`aggregate_manifest`] opens a sealed manifest and streams it through the
//! constant-memory [`ManifestBatchReader`]; [`aggregate_entries`] runs the same
//! accumulation over an in-memory entry iterator (used by the determinism
//! tests, with no Parquet fixture). Both funnel through [`Aggregator`] so the
//! file-backed and in-memory paths share one code path — and one determinism
//! basis.

use attestrum_core::{Modality, Result, SourceType};
use attestrum_manifest::{ManifestBatchReader, ManifestEntry};
use attestrum_merkle::merkle_root;
use std::collections::BTreeMap;
use std::path::Path;

/// Count + byte weight accumulated for one categorical bucket.
#[derive(Clone, Copy, Default)]
pub struct Weight {
    pub count: u64,
    pub bytes: u64,
}

/// One categorical dimension's histogram over the included documents. A `None`
/// source value folds into the `"unspecified"` bucket but is **not** counted in
/// `specified_documents` / `specified_bytes`, so coverage stays honest.
#[derive(Default)]
pub struct DimensionStats {
    pub buckets: BTreeMap<String, Weight>,
    pub specified_documents: u64,
    pub specified_bytes: u64,
}

impl DimensionStats {
    /// Add a known (non-null) value: bumps the bucket and the coverage counters.
    fn add_known(&mut self, key: &str, bytes: u64) {
        let w = self.buckets.entry(key.to_string()).or_default();
        w.count += 1;
        w.bytes += bytes;
        self.specified_documents += 1;
        self.specified_bytes += bytes;
    }

    /// Add an optional value: `Some` is a known value, `None` lands in the
    /// `"unspecified"` bucket without counting toward coverage.
    fn add_opt(&mut self, key: Option<String>, bytes: u64) {
        match key {
            Some(k) => self.add_known(&k, bytes),
            None => {
                let w = self.buckets.entry(UNSPECIFIED.to_string()).or_default();
                w.count += 1;
                w.bytes += bytes;
            }
        }
    }
}

/// Bucket key for a `None` optional dimension. No SPDX id, BCP-47 tag, or
/// `SourceType` variant collides with this literal.
pub const UNSPECIFIED: &str = "unspecified";

/// The full composition summary of one sealed corpus.
pub struct Composition {
    /// BLAKE3 Merkle root over every row's `document_id` in canonical order —
    /// byte-identical to the root the build pipeline sealed.
    pub merkle_root: [u8; 32],
    pub total_documents: u64,
    pub included_documents: u64,
    pub excluded_documents: u64,
    pub total_bytes: u64,
    pub included_bytes: u64,
    pub modality: DimensionStats,
    pub source_type: DimensionStats,
    pub license_spdx: DimensionStats,
    pub language: DimensionStats,
}

/// Running accumulator shared by the file-backed and in-memory entry paths.
#[derive(Default)]
struct Aggregator {
    leaves: Vec<[u8; 32]>,
    total_documents: u64,
    included_documents: u64,
    excluded_documents: u64,
    total_bytes: u64,
    included_bytes: u64,
    modality: DimensionStats,
    source_type: DimensionStats,
    license_spdx: DimensionStats,
    language: DimensionStats,
}

impl Aggregator {
    fn add(&mut self, e: &ManifestEntry) {
        // Every row is a Merkle leaf — included or not — matching how the
        // build pipeline computes the sealed root.
        self.leaves.push(e.document_id);
        self.total_documents += 1;
        self.total_bytes += e.size_bytes;

        if !e.included {
            self.excluded_documents += 1;
            return;
        }
        self.included_documents += 1;
        self.included_bytes += e.size_bytes;

        let bytes = e.size_bytes;
        self.modality.add_known(modality_key(e.modality), bytes);
        self.source_type
            .add_opt(e.source_type.map(|s| source_type_key(s).to_string()), bytes);
        self.license_spdx.add_opt(e.license_spdx.clone(), bytes);
        self.language.add_opt(e.language.clone(), bytes);
    }

    fn finish(self) -> Composition {
        Composition {
            merkle_root: merkle_root(&self.leaves),
            total_documents: self.total_documents,
            included_documents: self.included_documents,
            excluded_documents: self.excluded_documents,
            total_bytes: self.total_bytes,
            included_bytes: self.included_bytes,
            modality: self.modality,
            source_type: self.source_type,
            license_spdx: self.license_spdx,
            language: self.language,
        }
    }
}

/// Aggregate a sealed manifest into a [`Composition`] by streaming it through
/// the constant-memory batch reader.
pub fn aggregate_manifest(path: &Path) -> Result<Composition> {
    let reader = ManifestBatchReader::open(path)?;
    let mut acc = Aggregator::default();
    for batch in reader {
        for entry in batch? {
            acc.add(&entry);
        }
    }
    Ok(acc.finish())
}

/// Aggregate an in-memory entry stream into a [`Composition`]. Same accumulation
/// as [`aggregate_manifest`], no Parquet I/O — the determinism tests use this.
pub fn aggregate_entries<'a, I>(entries: I) -> Composition
where
    I: IntoIterator<Item = &'a ManifestEntry>,
{
    let mut acc = Aggregator::default();
    for entry in entries {
        acc.add(entry);
    }
    acc.finish()
}

/// Stable lowercase token for a [`Modality`].
fn modality_key(m: Modality) -> &'static str {
    match m {
        Modality::Text => "text",
        Modality::Image => "image",
        Modality::Audio => "audio",
        Modality::Video => "video",
        Modality::Pdf => "pdf",
        Modality::Other => "other",
    }
}

/// Stable lowercase token for a [`SourceType`].
fn source_type_key(s: SourceType) -> &'static str {
    match s {
        SourceType::Crawl => "crawl",
        SourceType::PublicDataset => "public_dataset",
        SourceType::PrivateLicensed => "private_licensed",
        SourceType::User => "user",
        SourceType::Synthetic => "synthetic",
        SourceType::Other => "other",
    }
}
