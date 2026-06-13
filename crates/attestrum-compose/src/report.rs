//! Report assembly and serialization.
//!
//! The canonical `report.json` is produced through
//! [`attestrum_attest::deterministic_json`] — the workspace's single sanctioned
//! sort-then-serialize primitive — so the bytes are a pure function of the
//! manifest. All keyed aggregates are `BTreeMap`s and all percentages are
//! 6-decimal-rounded, matching `attestrum diff` / `attestrum decontaminate`.

use crate::aggregate::{Composition, DimensionStats};
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
    pub composition: CompositionSection,
}

#[derive(Serialize)]
pub struct CorpusSection {
    pub manifest: String,
    /// Lowercase-hex BLAKE3 Merkle root over every `document_id` — the sealed
    /// corpus root.
    pub merkle_root: String,
    pub documents: u64,
    pub included: u64,
    pub excluded: u64,
    pub total_bytes: u64,
    pub included_bytes: u64,
}

#[derive(Serialize)]
pub struct CompositionSection {
    pub modality: Dimension,
    pub source_type: Dimension,
    pub license_spdx: Dimension,
    pub language: Dimension,
}

#[derive(Serialize)]
pub struct Dimension {
    pub buckets: BTreeMap<String, Bucket>,
    pub coverage: Coverage,
}

#[derive(Serialize)]
pub struct Bucket {
    pub count: u64,
    pub bytes: u64,
    /// Fraction of included documents (6-decimal rounded).
    pub count_pct: f64,
    /// Fraction of included bytes (6-decimal rounded).
    pub bytes_pct: f64,
}

#[derive(Serialize)]
pub struct Coverage {
    /// Included documents carrying a non-null value for this dimension.
    pub specified_documents: u64,
    pub specified_pct_by_count: f64,
    pub specified_pct_by_bytes: f64,
}

/// Build the report from an aggregated [`Composition`]. `manifest` is the
/// caller-facing manifest label (a path string); pass a fixed label in tests so
/// the golden is machine-independent.
pub fn build(manifest: String, comp: &Composition, timestamp: Option<String>) -> Report {
    let inc_count = comp.included_documents;
    let inc_bytes = comp.included_bytes;
    let dim = |d: &DimensionStats| -> Dimension {
        let buckets = d
            .buckets
            .iter()
            .map(|(k, w)| {
                (
                    k.clone(),
                    Bucket {
                        count: w.count,
                        bytes: w.bytes,
                        count_pct: pct(w.count, inc_count),
                        bytes_pct: pct(w.bytes, inc_bytes),
                    },
                )
            })
            .collect();
        Dimension {
            buckets,
            coverage: Coverage {
                specified_documents: d.specified_documents,
                specified_pct_by_count: pct(d.specified_documents, inc_count),
                specified_pct_by_bytes: pct(d.specified_bytes, inc_bytes),
            },
        }
    };

    Report {
        tool: "attestrum-compose".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
        corpus: CorpusSection {
            manifest,
            merkle_root: hex_64(&comp.merkle_root),
            documents: comp.total_documents,
            included: comp.included_documents,
            excluded: comp.excluded_documents,
            total_bytes: comp.total_bytes,
            included_bytes: comp.included_bytes,
        },
        composition: CompositionSection {
            modality: dim(&comp.modality),
            source_type: dim(&comp.source_type),
            license_spdx: dim(&comp.license_spdx),
            language: dim(&comp.language),
        },
    }
}

impl Report {
    /// Serialize to canonical JSON (recursive key sort, compact) with a single
    /// trailing newline. Bytes are a pure function of the manifest.
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
            "# attestrum compose — training-content summary (v{})\n\n",
            self.version
        );
        if let Some(ts) = &self.timestamp {
            let _ = writeln!(md, "- generated: {ts}");
        }
        let c = &self.corpus;
        let _ = writeln!(md, "- manifest: {}", c.manifest);
        let _ = writeln!(md, "- corpus root (BLAKE3 Merkle): `{}`", c.merkle_root);
        let _ = writeln!(
            md,
            "- documents: {} ({} included · {} excluded)",
            c.documents, c.included, c.excluded
        );
        let _ = writeln!(md, "- included size: {} bytes\n", c.included_bytes);

        render_dimension(&mut md, "Modality", &self.composition.modality);
        render_dimension(&mut md, "Source type", &self.composition.source_type);
        render_dimension(&mut md, "License (SPDX)", &self.composition.license_spdx);
        render_dimension(&mut md, "Language", &self.composition.language);
        md
    }
}

/// Render one dimension as a prevalence-sorted Markdown table with a coverage
/// header. Sort is `(count desc, key asc)` — deterministic, reads best-first.
fn render_dimension(md: &mut String, title: &str, dim: &Dimension) {
    let _ = write!(
        md,
        "## {} (coverage {:.1}% by count · {:.1}% by bytes)\n\n",
        title,
        dim.coverage.specified_pct_by_count * 100.0,
        dim.coverage.specified_pct_by_bytes * 100.0
    );
    md.push_str("| bucket | docs | % docs | bytes | % bytes |\n");
    md.push_str("|---|---:|---:|---:|---:|\n");
    let mut rows: Vec<(&String, &Bucket)> = dim.buckets.iter().collect();
    rows.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
    for (key, b) in rows {
        let _ = writeln!(
            md,
            "| {} | {} | {:.1}% | {} | {:.1}% |",
            key,
            b.count,
            b.count_pct * 100.0,
            b.bytes,
            b.bytes_pct * 100.0
        );
    }
    md.push('\n');
}

/// Fraction `num/denom` rounded to 6 decimals; 0.0 when `denom == 0`.
fn pct(num: u64, denom: u64) -> f64 {
    if denom == 0 {
        0.0
    } else {
        ((num as f64 / denom as f64) * 1_000_000.0).round() / 1_000_000.0
    }
}

/// Lowercase hex of a 32-byte root.
fn hex_64(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        let _ = write!(s, "{byte:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::{Composition, DimensionStats, Weight};
    use std::collections::BTreeMap;

    fn dim(
        pairs: &[(&str, u64, u64)],
        specified_docs: u64,
        specified_bytes: u64,
    ) -> DimensionStats {
        let mut buckets: BTreeMap<String, Weight> = BTreeMap::new();
        for (k, count, bytes) in pairs {
            buckets.insert(
                (*k).to_string(),
                Weight {
                    count: *count,
                    bytes: *bytes,
                },
            );
        }
        DimensionStats {
            buckets,
            specified_documents: specified_docs,
            specified_bytes,
        }
    }

    fn sample() -> Composition {
        Composition {
            merkle_root: [0u8; 32],
            total_documents: 3,
            included_documents: 2,
            excluded_documents: 1,
            total_bytes: 600,
            included_bytes: 300,
            modality: dim(&[("text", 2, 300)], 2, 300),
            source_type: dim(&[("crawl", 1, 100), ("unspecified", 1, 200)], 1, 100),
            license_spdx: dim(&[("unspecified", 2, 300)], 0, 0),
            language: dim(&[("en", 2, 300)], 2, 300),
        }
    }

    #[test]
    fn json_is_canonical_and_newline_terminated() {
        let report = build("m.parquet".into(), &sample(), None);
        let json = report.to_json().expect("serialize");
        assert!(json.ends_with("}\n"));
        // deterministic_json sorts keys: "composition" precedes "corpus".
        let comp = json.find("\"composition\"").expect("composition key");
        let corp = json.find("\"corpus\"").expect("corpus key");
        assert!(comp < corp, "object keys must be recursively sorted");
    }

    #[test]
    fn coverage_reflects_unspecified_exclusion() {
        let report = build("m.parquet".into(), &sample(), None);
        // license is entirely unspecified → 0% coverage; modality fully known.
        assert_eq!(
            report.composition.license_spdx.coverage.specified_documents,
            0
        );
        assert_eq!(
            report.composition.modality.coverage.specified_pct_by_count,
            1.0
        );
        // source_type: 1 of 2 included docs specified → 50%.
        assert_eq!(
            report
                .composition
                .source_type
                .coverage
                .specified_pct_by_count,
            0.5
        );
    }

    #[test]
    fn timestamp_is_rendered_when_supplied() {
        let report = build("m.parquet".into(), &sample(), Some("2026-06-13".into()));
        assert!(report.to_markdown().contains("- generated: 2026-06-13"));
    }

    #[test]
    fn empty_corpus_has_zero_percentages_not_nan() {
        let empty = Composition {
            merkle_root: [0u8; 32],
            total_documents: 0,
            included_documents: 0,
            excluded_documents: 0,
            total_bytes: 0,
            included_bytes: 0,
            modality: DimensionStats::default(),
            source_type: DimensionStats::default(),
            license_spdx: DimensionStats::default(),
            language: DimensionStats::default(),
        };
        let report = build("m.parquet".into(), &empty, None);
        assert_eq!(
            report.composition.modality.coverage.specified_pct_by_count,
            0.0
        );
        let json = report.to_json().expect("serialize");
        assert!(!json.contains("NaN"));
    }
}
