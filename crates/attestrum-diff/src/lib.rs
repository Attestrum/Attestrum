//! `attestrum-diff` — read-only, unsigned corpus-version delta between two
//! already-sealed manifests. The corpus-evolution sibling of `attestrum
//! inspect` (which reasons about one corpus): point it at two sealed
//! `manifest.parquet` states and it reports what changed, in a byte-reproducible
//! report.
//!
//! # Identity model (stated plainly — the report declares it)
//!
//! A manifest's `document_id` **is** the BLAKE3 content hash, and rows are
//! written in canonical `(document_id, occurrence_index)` sort order. So the
//! diff is a **merge-join of two sorted streams**:
//!
//! - **unchanged** — a `document_id` present in both versions
//! - **removed** — present in the old version only
//! - **added** — present in the new version only
//! - **multiset shift** — same `document_id`, different occurrence count (a
//!   document that appeared 3× now appears 1×)
//!
//! There is **no "modified" category**: in a content-addressed manifest a
//! changed document is simply a removed old hash plus an added new hash, with
//! nothing linking them (no caller-stable id exists in the schema). The report
//! states this mode rather than fuzzy-matching a "modified" verdict.
//!
//! # Streaming
//!
//! [`compare`] consumes two iterators of [`ManifestEntry`] in canonical sort
//! order — typically a flattened [`attestrum_manifest::ManifestBatchReader`] per
//! side — and walks them in lockstep. Only one group per side is held at a time;
//! the per-side leaf-digest vectors (needed for the Merkle root of each
//! endpoint) are the dominant allocation, the same memory envelope `attestrum
//! merge` accepts. It never materializes either manifest in full.
//!
//! # Determinism
//!
//! Same two manifests → byte-identical report on any machine. The walk proceeds
//! in `document_id` order, every map is a [`BTreeMap`], every example list is
//! sorted by construction (we collect in ascending `document_id` order and stop
//! at [`MAX_EXAMPLES`]), shares are rounded to 6 decimals for stable float
//! formatting, and JSON rendering rides the shared [`attestrum_attest::
//! deterministic_json`] canonical-key primitive. No wall-clock — a timestamp is
//! embedded only when the caller supplies one, verbatim.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::iter::Peekable;

use attestrum_core::{AttestrumError, Modality, Result, SourceType};
use attestrum_manifest::ManifestEntry;
use attestrum_merkle::merkle_root;
use serde::Serialize;

/// Report-shape version. A plain tag, deliberately **not** a predicate-type URI
/// — this is an unsigned report, outside the protected attestation perimeter.
pub const REPORT_VERSION: &str = "attestrum-diff-report/0.1";

/// The identity mode embedded in every report.
pub const IDENTITY_MODE: &str = "content-addressed multiset over BLAKE3 document_id; \
no \"modified\" category (a changed document is a removed old hash plus an added new hash)";

/// Features deliberately out of scope for the unsigned read-only diff, declared
/// verbatim in the report so it is honest about its own limits.
pub const DEFERRED: [&str; 3] = [
    "no \"modified\" category — would require a caller-stable document id (a protected manifest-schema change)",
    "no near-duplicate-rate delta — per-document fingerprints are not persisted in the manifest",
    "no signed corpus-delta predicate — frozen-on-first-use; deferred behind the high-stakes-decision protocol",
];

/// Cap on the number of example ids surfaced per category. The full counts are
/// always exact; only the example lists are truncated.
pub const MAX_EXAMPLES: usize = 16;

const UNSPECIFIED: &str = "(unspecified)";

/// The full delta report between two corpus versions.
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub report_version: &'static str,
    pub identity_mode: &'static str,
    pub deferred: Vec<&'static str>,
    /// Embedded verbatim only when the caller supplies one (Reproducible-Builds
    /// style); absent otherwise so the report carries no wall-clock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub old: VersionSummary,
    pub new: VersionSummary,
    pub delta: Delta,
}

/// Per-version summary, accumulated during the merge-join walk.
#[derive(Debug, Clone, Serialize)]
pub struct VersionSummary {
    /// BLAKE3 Merkle root of this version (lowercase hex) — the cryptographic
    /// name of the endpoint.
    pub merkle_root: String,
    /// Total rows, including duplicate occurrences.
    pub documents: u64,
    /// Distinct `document_id` values.
    pub distinct_documents: u64,
    /// `documents - distinct_documents`.
    pub exact_duplicate_documents: u64,
    pub total_bytes: u64,
    pub by_modality: BTreeMap<String, u64>,
    pub by_source_type: BTreeMap<String, u64>,
    pub by_source_dataset_id: BTreeMap<String, u64>,
    pub by_license_spdx: BTreeMap<String, u64>,
    pub by_language: BTreeMap<String, u64>,
}

/// The delta between the two versions.
#[derive(Debug, Clone, Serialize)]
pub struct Delta {
    pub added: u64,
    pub removed: u64,
    pub unchanged: u64,
    /// First [`MAX_EXAMPLES`] added ids (lowercase hex), in ascending order.
    pub added_examples: Vec<String>,
    /// First [`MAX_EXAMPLES`] removed ids (lowercase hex), in ascending order.
    pub removed_examples: Vec<String>,
    /// Documents present in both versions whose occurrence count changed.
    pub multiset_shifts: Vec<MultisetShift>,
    /// `dimension -> label -> (old share, new share)`, over the union of labels.
    pub composition_shift: BTreeMap<String, BTreeMap<String, ShareShift>>,
}

/// One document whose multiplicity changed between versions.
#[derive(Debug, Clone, Serialize)]
pub struct MultisetShift {
    pub document_id: String,
    pub old_count: u64,
    pub new_count: u64,
}

/// A per-label composition share before and after, each in `[0, 1]`, rounded to
/// 6 decimals.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ShareShift {
    pub old: f64,
    pub new: f64,
}

/// Compare two corpus versions, each given as an iterator of [`ManifestEntry`]
/// in canonical `(document_id, occurrence_index)` order. `timestamp`, when
/// `Some`, is embedded in the report verbatim.
///
/// Errors propagate a stream read failure, or [`AttestrumError::Internal`] if a
/// side is not canonically sorted (the merge-join's correctness precondition).
pub fn compare<A, B>(old: A, new: B, timestamp: Option<String>) -> Result<DiffReport>
where
    A: Iterator<Item = Result<ManifestEntry>>,
    B: Iterator<Item = Result<ManifestEntry>>,
{
    let mut ga = GroupReader::new(old);
    let mut gb = GroupReader::new(new);

    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut unchanged: u64 = 0;
    let mut added_examples: Vec<String> = Vec::new();
    let mut removed_examples: Vec<String> = Vec::new();
    let mut multiset_shifts: Vec<MultisetShift> = Vec::new();

    let mut a = ga.next_group()?;
    let mut b = gb.next_group()?;
    loop {
        match (a, b) {
            (None, None) => break,
            (Some((da, _)), None) => {
                removed += 1;
                push_example(&mut removed_examples, da);
                a = ga.next_group()?;
            }
            (None, Some((db, _))) => {
                added += 1;
                push_example(&mut added_examples, db);
                b = gb.next_group()?;
            }
            (Some((da, ca)), Some((db, cb))) => match da.cmp(&db) {
                Ordering::Less => {
                    removed += 1;
                    push_example(&mut removed_examples, da);
                    a = ga.next_group()?;
                }
                Ordering::Greater => {
                    added += 1;
                    push_example(&mut added_examples, db);
                    b = gb.next_group()?;
                }
                Ordering::Equal => {
                    unchanged += 1;
                    if ca != cb && multiset_shifts.len() < MAX_EXAMPLES {
                        multiset_shifts.push(MultisetShift {
                            document_id: hex_64(&da),
                            old_count: ca,
                            new_count: cb,
                        });
                    }
                    a = ga.next_group()?;
                    b = gb.next_group()?;
                }
            },
        }
    }

    let old = ga.into_summary();
    let new = gb.into_summary();
    let composition_shift = composition_shift(&old, &new);

    Ok(DiffReport {
        report_version: REPORT_VERSION,
        identity_mode: IDENTITY_MODE,
        deferred: DEFERRED.to_vec(),
        timestamp,
        old,
        new,
        delta: Delta {
            added,
            removed,
            unchanged,
            added_examples,
            removed_examples,
            multiset_shifts,
            composition_shift,
        },
    })
}

/// Render the canonical, byte-deterministic JSON report.
pub fn render_json(report: &DiffReport) -> std::result::Result<String, serde_json::Error> {
    attestrum_attest::deterministic_json(report)
}

/// Render a concise human-readable summary (printed to stdout by the CLI).
pub fn render_summary(report: &DiffReport) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let d = &report.delta;
    let _ = writeln!(s, "corpus diff ({})", report.report_version);
    let _ = writeln!(s, "identity mode: {}", report.identity_mode);
    let _ = writeln!(
        s,
        "old: {}  ({} docs, {} distinct, {} bytes)",
        report.old.merkle_root,
        report.old.documents,
        report.old.distinct_documents,
        report.old.total_bytes
    );
    let _ = writeln!(
        s,
        "new: {}  ({} docs, {} distinct, {} bytes)",
        report.new.merkle_root,
        report.new.documents,
        report.new.distinct_documents,
        report.new.total_bytes
    );
    let _ = writeln!(
        s,
        "delta: added {} · removed {} · unchanged {} · multiset-shifts {}",
        d.added,
        d.removed,
        d.unchanged,
        d.multiset_shifts.len()
    );
    s
}

// ----------------------------------------------------------------------------
// Internals
// ----------------------------------------------------------------------------

fn push_example(v: &mut Vec<String>, id: [u8; 32]) {
    if v.len() < MAX_EXAMPLES {
        v.push(hex_64(&id));
    }
}

/// Walks a canonically-sorted entry stream, yielding one `(document_id, count)`
/// group per distinct id in ascending order while folding every row into a
/// [`Summary`] accumulator. Owns its accumulator; surrender it with
/// [`GroupReader::into_summary`] once the stream is drained.
struct GroupReader<I: Iterator<Item = Result<ManifestEntry>>> {
    iter: Peekable<I>,
    acc: Summary,
    last_id: Option<[u8; 32]>,
}

impl<I: Iterator<Item = Result<ManifestEntry>>> GroupReader<I> {
    fn new(iter: I) -> Self {
        Self {
            iter: iter.peekable(),
            acc: Summary::new(),
            last_id: None,
        }
    }

    fn next_group(&mut self) -> Result<Option<([u8; 32], u64)>> {
        let group_id = match self.iter.peek() {
            None => return Ok(None),
            Some(Ok(e)) => e.document_id,
            Some(Err(_)) => return Err(self.take_err()),
        };
        if let Some(prev) = self.last_id {
            if group_id < prev {
                return Err(AttestrumError::Internal(format!(
                    "manifest not in canonical (document_id, occurrence_index) order: \
                     {} sorts before {}",
                    hex_64(&group_id),
                    hex_64(&prev)
                )));
            }
        }
        let mut count: u64 = 0;
        loop {
            match self.iter.peek() {
                Some(Ok(e)) if e.document_id == group_id => {
                    let e = self.take_ok();
                    self.acc.fold_row(&e);
                    count += 1;
                }
                Some(Ok(_)) | None => break,
                Some(Err(_)) => return Err(self.take_err()),
            }
        }
        self.acc.distinct_documents += 1;
        self.last_id = Some(group_id);
        Ok(Some((group_id, count)))
    }

    /// Consume the next item, which the caller has already peeked as `Ok`.
    fn take_ok(&mut self) -> ManifestEntry {
        self.iter.next().expect("peeked Some").expect("peeked Ok")
    }

    /// Consume and surface the next item, which the caller has already peeked as
    /// `Err`.
    fn take_err(&mut self) -> AttestrumError {
        self.iter
            .next()
            .expect("peeked Some")
            .expect_err("peeked Err")
    }

    fn into_summary(self) -> VersionSummary {
        self.acc.finish()
    }
}

/// Mutable per-version accumulator. Carries the running counts and the leaf
/// vector needed for the endpoint's Merkle root.
struct Summary {
    documents: u64,
    distinct_documents: u64,
    total_bytes: u64,
    leaves: Vec<[u8; 32]>,
    by_modality: BTreeMap<String, u64>,
    by_source_type: BTreeMap<String, u64>,
    by_source_dataset_id: BTreeMap<String, u64>,
    by_license_spdx: BTreeMap<String, u64>,
    by_language: BTreeMap<String, u64>,
}

impl Summary {
    fn new() -> Self {
        Self {
            documents: 0,
            distinct_documents: 0,
            total_bytes: 0,
            leaves: Vec::new(),
            by_modality: BTreeMap::new(),
            by_source_type: BTreeMap::new(),
            by_source_dataset_id: BTreeMap::new(),
            by_license_spdx: BTreeMap::new(),
            by_language: BTreeMap::new(),
        }
    }

    fn fold_row(&mut self, e: &ManifestEntry) {
        self.documents += 1;
        self.total_bytes += e.size_bytes;
        self.leaves.push(e.document_id);
        bump(&mut self.by_modality, modality_label(e.modality));
        bump(&mut self.by_source_type, source_type_label(e.source_type));
        bump(
            &mut self.by_source_dataset_id,
            opt_label(&e.source_dataset_id),
        );
        bump(&mut self.by_license_spdx, opt_label(&e.license_spdx));
        bump(&mut self.by_language, opt_label(&e.language));
    }

    fn finish(self) -> VersionSummary {
        VersionSummary {
            merkle_root: hex_64(&merkle_root(&self.leaves)),
            documents: self.documents,
            distinct_documents: self.distinct_documents,
            exact_duplicate_documents: self.documents - self.distinct_documents,
            total_bytes: self.total_bytes,
            by_modality: self.by_modality,
            by_source_type: self.by_source_type,
            by_source_dataset_id: self.by_source_dataset_id,
            by_license_spdx: self.by_license_spdx,
            by_language: self.by_language,
        }
    }
}

fn bump(m: &mut BTreeMap<String, u64>, label: String) {
    *m.entry(label).or_insert(0) += 1;
}

fn composition_shift(
    old: &VersionSummary,
    new: &VersionSummary,
) -> BTreeMap<String, BTreeMap<String, ShareShift>> {
    let (ot, nt) = (old.documents, new.documents);
    let mut out = BTreeMap::new();
    out.insert(
        "modality".to_string(),
        shift_dim(&old.by_modality, ot, &new.by_modality, nt),
    );
    out.insert(
        "source_type".to_string(),
        shift_dim(&old.by_source_type, ot, &new.by_source_type, nt),
    );
    out.insert(
        "source_dataset_id".to_string(),
        shift_dim(&old.by_source_dataset_id, ot, &new.by_source_dataset_id, nt),
    );
    out.insert(
        "license_spdx".to_string(),
        shift_dim(&old.by_license_spdx, ot, &new.by_license_spdx, nt),
    );
    out.insert(
        "language".to_string(),
        shift_dim(&old.by_language, ot, &new.by_language, nt),
    );
    out
}

fn shift_dim(
    old: &BTreeMap<String, u64>,
    old_total: u64,
    new: &BTreeMap<String, u64>,
    new_total: u64,
) -> BTreeMap<String, ShareShift> {
    let labels: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let mut m = BTreeMap::new();
    for label in labels {
        m.insert(
            label.clone(),
            ShareShift {
                old: share(old.get(label).copied().unwrap_or(0), old_total),
                new: share(new.get(label).copied().unwrap_or(0), new_total),
            },
        );
    }
    m
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        round6(part as f64 / whole as f64)
    }
}

/// Round to 6 decimals so float formatting is stable across platforms.
fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

fn modality_label(m: Modality) -> String {
    match m {
        Modality::Text => "text",
        Modality::Image => "image",
        Modality::Audio => "audio",
        Modality::Video => "video",
        Modality::Pdf => "pdf",
        Modality::Other => "other",
    }
    .to_string()
}

fn source_type_label(s: Option<SourceType>) -> String {
    match s {
        None => UNSPECIFIED,
        Some(SourceType::Crawl) => "crawl",
        Some(SourceType::PublicDataset) => "public_dataset",
        Some(SourceType::PrivateLicensed) => "private_licensed",
        Some(SourceType::User) => "user",
        Some(SourceType::Synthetic) => "synthetic",
        Some(SourceType::Other) => "other",
    }
    .to_string()
}

fn opt_label(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| UNSPECIFIED.to_string())
}

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use attestrum_manifest::ManifestSignals;

    /// Build an entry. `id` becomes every byte of `document_id`, so ids sort by
    /// the numeric value of `id`.
    #[allow(clippy::too_many_arguments)]
    fn entry(
        id: u8,
        occurrence_index: u32,
        modality: Modality,
        source_type: Option<SourceType>,
        dataset: Option<&str>,
        license: Option<&str>,
        language: Option<&str>,
        size_bytes: u64,
    ) -> ManifestEntry {
        ManifestEntry {
            document_id: [id; 32],
            sha256: [0u8; 32],
            size_bytes,
            modality,
            mime_type: None,
            source_url: None,
            source_type,
            source_dataset_id: dataset.map(str::to_string),
            registered_domain: None,
            license_spdx: license.map(str::to_string),
            language: language.map(str::to_string),
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
            chunk_refs: None,
            input_ordinal: id as u64,
            occurrence_index,
        }
    }

    fn text(id: u8) -> ManifestEntry {
        entry(id, 0, Modality::Text, None, None, None, None, 10)
    }

    fn run(old: Vec<ManifestEntry>, new: Vec<ManifestEntry>) -> DiffReport {
        compare(old.into_iter().map(Ok), new.into_iter().map(Ok), None).expect("compare ok")
    }

    #[test]
    fn classifies_added_removed_unchanged() {
        // old {1,2,3} → new {1,2,4}: 4 added, 3 removed, 1 & 2 unchanged.
        let old = vec![text(1), text(2), text(3)];
        let new = vec![text(1), text(2), text(4)];
        let r = run(old, new);
        assert_eq!(
            (r.delta.added, r.delta.removed, r.delta.unchanged),
            (1, 1, 2)
        );
        assert_eq!(r.delta.added_examples, [hex_64(&[4u8; 32])]);
        assert_eq!(r.delta.removed_examples, [hex_64(&[3u8; 32])]);
        assert!(r.delta.multiset_shifts.is_empty());
    }

    #[test]
    fn identity_self_diff_is_all_unchanged() {
        let v = vec![text(1), text(2), text(5)];
        let r = run(v.clone(), v);
        assert_eq!(
            (r.delta.added, r.delta.removed, r.delta.unchanged),
            (0, 0, 3)
        );
        assert_eq!(r.old.merkle_root, r.new.merkle_root);
    }

    #[test]
    fn multiset_shift_when_occurrence_count_changes() {
        // id 1 appears 3× in old, 1× in new — unchanged (same content) but a
        // multiset shift; id 2 is a steady singleton.
        let old = vec![
            entry(1, 0, Modality::Text, None, None, None, None, 10),
            entry(1, 1, Modality::Text, None, None, None, None, 10),
            entry(1, 2, Modality::Text, None, None, None, None, 10),
            text(2),
        ];
        let new = vec![text(1), text(2)];
        let r = run(old, new);
        assert_eq!(
            (r.delta.added, r.delta.removed, r.delta.unchanged),
            (0, 0, 2)
        );
        assert_eq!(r.delta.multiset_shifts.len(), 1);
        let shift = &r.delta.multiset_shifts[0];
        assert_eq!(shift.document_id, hex_64(&[1u8; 32]));
        assert_eq!((shift.old_count, shift.new_count), (3, 1));
        // old: 4 rows / 2 distinct → 2 exact-duplicate documents.
        assert_eq!(r.old.documents, 4);
        assert_eq!(r.old.distinct_documents, 2);
        assert_eq!(r.old.exact_duplicate_documents, 2);
    }

    #[test]
    fn composition_shift_over_union_of_labels() {
        // old: 2 text/web; new: 1 text/web + 1 image/code.
        let old = vec![
            entry(1, 0, Modality::Text, None, Some("web"), None, None, 10),
            entry(2, 0, Modality::Text, None, Some("web"), None, None, 10),
        ];
        let new = vec![
            entry(1, 0, Modality::Text, None, Some("web"), None, None, 10),
            entry(2, 0, Modality::Image, None, Some("code"), None, None, 10),
        ];
        let r = run(old, new);
        let modality = &r.delta.composition_shift["modality"];
        assert_eq!(modality["text"].old, 1.0);
        assert_eq!(modality["text"].new, 0.5);
        assert_eq!(modality["image"].old, 0.0);
        assert_eq!(modality["image"].new, 0.5);
        let dataset = &r.delta.composition_shift["source_dataset_id"];
        assert_eq!(dataset["web"].old, 1.0);
        assert_eq!(dataset["web"].new, 0.5);
        assert_eq!(dataset["code"].new, 0.5);
    }

    #[test]
    fn empty_vs_empty_is_valid_and_zeroed() {
        let r = run(vec![], vec![]);
        assert_eq!(
            (r.delta.added, r.delta.removed, r.delta.unchanged),
            (0, 0, 0)
        );
        assert_eq!(r.old.documents, 0);
        assert_eq!(r.old.merkle_root, r.new.merkle_root);
    }

    #[test]
    fn unsorted_input_is_rejected() {
        // Descending document_id violates the canonical-order precondition.
        let old = vec![text(5), text(2)];
        let err = compare(
            old.into_iter().map(Ok),
            std::iter::empty::<Result<ManifestEntry>>(),
            None,
        )
        .expect_err("must reject unsorted input");
        assert!(matches!(err, AttestrumError::Internal(_)));
    }

    #[test]
    fn read_error_propagates() {
        let old: Vec<Result<ManifestEntry>> =
            vec![Ok(text(1)), Err(AttestrumError::Hash("boom".into()))];
        let err = compare(
            old.into_iter(),
            std::iter::empty::<Result<ManifestEntry>>(),
            None,
        )
        .expect_err("read error must surface");
        assert!(matches!(err, AttestrumError::Hash(_)));
    }

    #[test]
    fn json_render_is_stable_across_two_calls() {
        let old = vec![text(1), text(2)];
        let new = vec![text(2), text(3)];
        let r = run(old, new);
        let a = render_json(&r).unwrap();
        let b = render_json(&r).unwrap();
        assert_eq!(a, b);
        assert!(a.contains("\"identity_mode\""));
    }
}
