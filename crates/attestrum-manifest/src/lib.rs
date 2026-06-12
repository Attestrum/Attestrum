//! `attestrum-manifest` — Parquet manifest schema + writer + reader for Attestrum.
//!
//! Sprint 3 E2 + E2.5 shipped the pure-Rust type layer: [`ManifestEntry`],
//! [`ManifestSignals`], and the deterministic-ordering helpers
//! [`assign_input_ordinals`], [`assign_occurrence_indices`], and
//! [`sort_entries`]. Sprint 3 E3 ships [`io::write_manifest`],
//! [`io::read_manifest`], [`io::read_manifest_metadata`], and the PROTECTED
//! Arrow schema + Parquet writer config (PARQUET_1_0, ZSTD-3, dict OFF
//! global, stats OFF global, raw Int8 enums, raw Int64 timestamps,
//! `created_by` pinned, schema_version + writer_profile in KeyValue
//! metadata). The `.attestrum/manifests/` layout subdir is consumed by callers
//! (any file path) but the schema + writer config are PROTECTED-system per
//! CLAUDE.md §4.
//!
//! See `docs/diagrams/sprint-3/manifest-schema.md` for the canonical schema:
//! 16 columns from BUILD-PLAN §4.2 plus TWO Sprint 3 binding columns —
//! `input_ordinal` (pre-parallel positional id) and `occurrence_index`
//! (per-digest multiset rank). The pair makes the multiset binding
//! independently auditable: a verifier can sort `(document_id, input_ordinal)`
//! and recompute `occurrence_index` to confirm Attestrum assigned it correctly.
//! Founder-approved per E3 pre-implementation cross-check, 2026-05-24
//! (responses preserved at `~/Downloads/attestrum-e3/`).

use std::collections::HashMap;

use attestrum_core::{Modality, SourceType};
use serde::{Deserialize, Serialize};

pub mod io;

pub use io::{
    arrow_schema, manifest_row_count, read_manifest, read_manifest_metadata, write_manifest,
    writer_properties, ManifestBatchReader, ManifestWriter, CREATED_BY, SCHEMA_VERSION,
    WRITER_PROFILE,
};

// ============================================================================
// ManifestSignals (the embedded signals STRUCT column from BUILD-PLAN §4.2)
// ============================================================================

/// Per-document aggregation of every signal parser's output. Stored as the
/// `signals` Parquet STRUCT column of [`ManifestEntry`].
///
/// `bool` fields encode simple allow/disallow flips. `Option<String>` fields
/// carry the free-form value when present (e.g., a TDMRep policy URL, an
/// IPTC PLUS DMI vocabulary URI). `tdmrep_reservation` is signed for the
/// tri-state encoding `-1 = unset, 0 = allow, 1 = reserve` per TDMRep wire
/// format (BUILD-PLAN §4.2).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSignals {
    pub robots_disallow: bool,
    pub robots_user_agent: Option<String>,
    pub ai_txt_disallow: bool,
    pub tdmrep_reservation: i8,
    pub tdmrep_policy_url: Option<String>,
    pub aipref_usage_pref: Option<String>,
    pub iptc_plus_dmi: Option<String>,
    pub c2pa_training_mining: Option<String>,
    pub rsl_permits: Option<String>,
    pub liccium_tdmai_iscc: Option<String>,
    pub liccium_tdmai_allow: Option<bool>,
    pub cloudflare_ai_train: Option<String>,
}

// ============================================================================
// ManifestEntry (one row of the Parquet manifest)
// ============================================================================

/// One row of the Attestrum Parquet manifest. 16 columns from BUILD-PLAN §4.2
/// plus two Sprint 3 binding columns: `input_ordinal` and `occurrence_index`.
///
/// **Sort order** (canonical Parquet on-disk order):
/// `(document_id, occurrence_index)`. The Merkle tree's leaves are emitted
/// in this order so the root is a deterministic function of the corpus as a
/// multiset.
///
/// **Binding columns**:
/// - `input_ordinal` is the row's position in the original input list (0..N).
///   Assigned by [`assign_input_ordinals`] BEFORE the parallel hashing phase.
///   It's the stable per-document positional id that workers carry through
///   the pipeline without modification, and lets an external verifier
///   independently reconstruct `occurrence_index` from the manifest alone.
/// - `occurrence_index` is the row's 0-based rank within the multiset of
///   rows sharing the same `document_id`. Assigned by
///   [`assign_occurrence_indices`] using input ordering (which is identical
///   to `input_ordinal` ordering by construction). It's the deterministic
///   tie-break inside any group of identical digests; an auditor with the
///   manifest + Merkle root can reconstruct exactly which leaf maps to which
///   row.
///
/// **Audit invariant**: post-`sort_entries`, within each consecutive group
/// sharing the same `document_id`, `occurrence_index` increases monotonically
/// from 0 AND equals the rank when the group is sorted by `input_ordinal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub document_id: [u8; 32],
    pub sha256: [u8; 32],
    pub size_bytes: u64,
    pub modality: Modality,
    pub mime_type: Option<String>,
    pub source_url: Option<String>,
    pub source_type: Option<SourceType>,
    pub source_dataset_id: Option<String>,
    pub registered_domain: Option<String>,
    pub license_spdx: Option<String>,
    pub language: Option<String>,
    pub fetched_at: Option<i64>,
    pub signals: ManifestSignals,
    pub included: bool,
    pub exclusion_reason: Option<String>,
    pub chunk_refs: Option<Vec<[u8; 32]>>,
    pub input_ordinal: u64,
    pub occurrence_index: u32,
}

// ============================================================================
// Deterministic ordering helpers
// ============================================================================

/// Walk `entries` in input order and assign each `input_ordinal` to its
/// 0-based position in the slice.
///
/// This is the FIRST step of the canonical pipeline:
///
/// ```text
/// assign_input_ordinals(entries)          // sets input_ordinal[i] = i as u64
/// assign_occurrence_indices(entries)      // sets occurrence_index per digest
/// // ... parallel hashing phase preserves input_ordinal + occurrence_index ...
/// sort_entries(entries)                   // canonical (document_id, occurrence_index) order
/// ```
///
/// The pair `(document_id, input_ordinal)` is unique by construction (every
/// row has a distinct position), so an external auditor reading the sealed
/// Parquet can independently:
///   1. Sort by `(document_id, input_ordinal)`, and
///   2. Walk each consecutive `document_id` group assigning a 0-based rank,
///   3. Assert the assigned rank equals the manifest's `occurrence_index`.
///
/// If that holds, the multiset Merkle binding is correct.
pub fn assign_input_ordinals(entries: &mut [ManifestEntry]) {
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.input_ordinal = i as u64;
    }
}

/// Walk `entries` in input order and assign each `occurrence_index` to its
/// 0-based ordinal among entries sharing the same `document_id`.
///
/// Multiset semantics: identical `document_id` values are NOT collapsed.
/// Three input entries with the same BLAKE3 digest end up with
/// `occurrence_index = 0, 1, 2` respectively in input order. This is what
/// binds each manifest row to a specific leaf in the multiset Merkle tree
/// built later by `attestrum-pipeline`.
///
/// Determinism: the HashMap is used only for digest-to-counter lookups.
/// Map iteration order is never observed; the result depends only on the
/// input slice's content + order, not on hash-randomization state.
///
/// **Call this BEFORE [`sort_entries`]** — the index records input ordering
/// and must be assigned before any re-ordering happens.
pub fn assign_occurrence_indices(entries: &mut [ManifestEntry]) {
    let mut counters: HashMap<[u8; 32], u32> = HashMap::new();
    for entry in entries.iter_mut() {
        let counter = counters.entry(entry.document_id).or_insert(0);
        entry.occurrence_index = *counter;
        *counter += 1;
    }
}

/// Sort `entries` in place by `(document_id, occurrence_index)`. This is the
/// canonical Parquet on-disk sort order from BUILD-PLAN §4.2; the Merkle
/// tree's leaves are emitted in this order so the root is a deterministic
/// function of the corpus.
///
/// **Always call [`assign_occurrence_indices`] BEFORE [`sort_entries`]** so
/// the tie-break is meaningful within any group of identical digests.
pub fn sort_entries(entries: &mut [ManifestEntry]) {
    entries.sort_by(|a, b| {
        a.document_id
            .cmp(&b.document_id)
            .then(a.occurrence_index.cmp(&b.occurrence_index))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn sample_entry(doc_byte: u8) -> ManifestEntry {
        ManifestEntry {
            document_id: digest(doc_byte),
            sha256: digest(doc_byte ^ 0xff),
            size_bytes: u64::from(doc_byte) * 100,
            modality: Modality::Text,
            mime_type: Some("text/plain".into()),
            source_url: Some(format!("file:///docs/doc-{doc_byte:02x}.txt")),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some("common-pile-mini".into()),
            registered_domain: None,
            license_spdx: Some("CC0-1.0".into()),
            language: Some("en".into()),
            fetched_at: Some(1_700_000_000),
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
            chunk_refs: None,
            input_ordinal: 0,
            occurrence_index: 0,
        }
    }

    #[test]
    fn manifest_signals_default_is_all_quiet() {
        let s = ManifestSignals::default();
        assert!(!s.robots_disallow);
        assert!(s.robots_user_agent.is_none());
        assert!(!s.ai_txt_disallow);
        assert_eq!(s.tdmrep_reservation, 0);
        assert!(s.tdmrep_policy_url.is_none());
        assert!(s.cloudflare_ai_train.is_none());
    }

    #[test]
    fn manifest_signals_round_trips_via_serde_json() {
        let original = ManifestSignals {
            robots_disallow: true,
            robots_user_agent: Some("GPTBot".into()),
            ai_txt_disallow: false,
            tdmrep_reservation: 1,
            tdmrep_policy_url: Some("https://example.com/policy".into()),
            aipref_usage_pref: Some("opt-out".into()),
            iptc_plus_dmi: None,
            c2pa_training_mining: None,
            rsl_permits: None,
            liccium_tdmai_iscc: None,
            liccium_tdmai_allow: Some(false),
            cloudflare_ai_train: Some("no".into()),
        };
        let s = serde_json::to_string(&original).unwrap();
        let back: ManifestSignals = serde_json::from_str(&s).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn manifest_entry_round_trips_via_serde_json() {
        let mut entry = sample_entry(0xab);
        entry.input_ordinal = 7;
        entry.occurrence_index = 3;
        let s = serde_json::to_string(&entry).unwrap();
        let back: ManifestEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn manifest_entry_round_trips_with_all_optionals_none() {
        let entry = ManifestEntry {
            document_id: digest(0x01),
            sha256: digest(0x02),
            size_bytes: 0,
            modality: Modality::Other,
            mime_type: None,
            source_url: None,
            source_type: None,
            source_dataset_id: None,
            registered_domain: None,
            license_spdx: None,
            language: None,
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: false,
            exclusion_reason: Some("no signal expressed a preference".into()),
            chunk_refs: None,
            input_ordinal: 42,
            occurrence_index: 0,
        };
        let s = serde_json::to_string(&entry).unwrap();
        let back: ManifestEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn manifest_entry_round_trips_with_chunk_refs() {
        let mut entry = sample_entry(0x10);
        entry.chunk_refs = Some(vec![digest(0x11), digest(0x12), digest(0x13)]);
        let s = serde_json::to_string(&entry).unwrap();
        let back: ManifestEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn assign_occurrence_indices_distinct_digests_all_get_zero() {
        let mut entries = vec![
            sample_entry(0x01),
            sample_entry(0x02),
            sample_entry(0x03),
            sample_entry(0x04),
            sample_entry(0x05),
        ];
        for entry in entries.iter_mut() {
            entry.occurrence_index = 0xdead_beef; // poisoned to prove the call overwrites
        }
        assign_occurrence_indices(&mut entries);
        for entry in &entries {
            assert_eq!(entry.occurrence_index, 0);
        }
    }

    #[test]
    fn assign_occurrence_indices_three_copies_of_same_doc_get_012() {
        let mut entries = vec![sample_entry(0xaa), sample_entry(0xaa), sample_entry(0xaa)];
        assign_occurrence_indices(&mut entries);
        assert_eq!(entries[0].occurrence_index, 0);
        assert_eq!(entries[1].occurrence_index, 1);
        assert_eq!(entries[2].occurrence_index, 2);
    }

    #[test]
    fn assign_occurrence_indices_interleaved_digests_count_per_digest_in_input_order() {
        // Input: A B A B A C → indices [0,0,1,1,2,0]
        let mut entries = vec![
            sample_entry(0xaa),
            sample_entry(0xbb),
            sample_entry(0xaa),
            sample_entry(0xbb),
            sample_entry(0xaa),
            sample_entry(0xcc),
        ];
        assign_occurrence_indices(&mut entries);
        let observed: Vec<u32> = entries.iter().map(|e| e.occurrence_index).collect();
        assert_eq!(observed, vec![0, 0, 1, 1, 2, 0]);
    }

    #[test]
    fn assign_occurrence_indices_empty_slice_is_no_op() {
        let mut entries: Vec<ManifestEntry> = Vec::new();
        assign_occurrence_indices(&mut entries);
        assert!(entries.is_empty());
    }

    #[test]
    fn sort_entries_orders_by_document_id_first() {
        let mut entries = vec![
            sample_entry(0x05),
            sample_entry(0x01),
            sample_entry(0x03),
            sample_entry(0x02),
            sample_entry(0x04),
        ];
        sort_entries(&mut entries);
        let ids: Vec<u8> = entries.iter().map(|e| e.document_id[0]).collect();
        assert_eq!(ids, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    }

    #[test]
    fn sort_entries_uses_occurrence_index_as_tiebreak() {
        let mut entries = vec![sample_entry(0xaa), sample_entry(0xaa), sample_entry(0xaa)];
        entries[0].occurrence_index = 2;
        entries[1].occurrence_index = 0;
        entries[2].occurrence_index = 1;
        sort_entries(&mut entries);
        let occs: Vec<u32> = entries.iter().map(|e| e.occurrence_index).collect();
        assert_eq!(occs, vec![0, 1, 2]);
    }

    #[test]
    fn assign_then_sort_produces_canonical_order_for_mixed_input() {
        // Input shuffled with duplicates: D A B A C B A
        let mut entries = vec![
            sample_entry(0xdd),
            sample_entry(0xaa),
            sample_entry(0xbb),
            sample_entry(0xaa),
            sample_entry(0xcc),
            sample_entry(0xbb),
            sample_entry(0xaa),
        ];
        assign_occurrence_indices(&mut entries);
        sort_entries(&mut entries);
        // Expected canonical order: lex by document_id[0], then by
        // occurrence_index (assigned in input order pre-sort):
        //   A.0, A.1, A.2, B.0, B.1, C.0, D.0
        let observed: Vec<(u8, u32)> = entries
            .iter()
            .map(|e| (e.document_id[0], e.occurrence_index))
            .collect();
        assert_eq!(
            observed,
            vec![
                (0xaa, 0),
                (0xaa, 1),
                (0xaa, 2),
                (0xbb, 0),
                (0xbb, 1),
                (0xcc, 0),
                (0xdd, 0),
            ]
        );
    }

    #[test]
    fn sort_entries_is_a_no_op_on_empty_slice() {
        let mut entries: Vec<ManifestEntry> = Vec::new();
        sort_entries(&mut entries);
        assert!(entries.is_empty());
    }

    // -----------------------------------------------------------------------
    // E2.5: input_ordinal + audit invariant
    // -----------------------------------------------------------------------

    #[test]
    fn assign_input_ordinals_counts_from_zero_in_slice_order() {
        let mut entries = vec![
            sample_entry(0x05),
            sample_entry(0x01),
            sample_entry(0x03),
            sample_entry(0x02),
        ];
        for entry in entries.iter_mut() {
            entry.input_ordinal = 0xdead_beef_cafe_babe; // poisoned
        }
        assign_input_ordinals(&mut entries);
        let observed: Vec<u64> = entries.iter().map(|e| e.input_ordinal).collect();
        assert_eq!(observed, vec![0, 1, 2, 3]);
    }

    #[test]
    fn assign_input_ordinals_empty_slice_is_no_op() {
        let mut entries: Vec<ManifestEntry> = Vec::new();
        assign_input_ordinals(&mut entries);
        assert!(entries.is_empty());
    }

    #[test]
    fn assign_input_ordinals_ignores_document_id_duplicates() {
        // Three copies of the same digest still get distinct input_ordinals.
        let mut entries = vec![sample_entry(0xaa), sample_entry(0xaa), sample_entry(0xaa)];
        assign_input_ordinals(&mut entries);
        let observed: Vec<u64> = entries.iter().map(|e| e.input_ordinal).collect();
        assert_eq!(observed, vec![0, 1, 2]);
    }

    #[test]
    fn input_ordinal_round_trips_via_serde_json() {
        let mut entry = sample_entry(0xab);
        entry.input_ordinal = u64::MAX - 1;
        let s = serde_json::to_string(&entry).unwrap();
        let back: ManifestEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back.input_ordinal, u64::MAX - 1);
        assert_eq!(back, entry);
    }

    #[test]
    fn audit_invariant_occurrence_index_equals_input_ordinal_rank_within_digest_group() {
        // Canonical pipeline: assign input_ordinal, assign occurrence_index,
        // sort. Then for each consecutive digest group post-sort, the entries
        // are sorted by occurrence_index AND by input_ordinal (since both
        // were assigned in the same input walk). This is the audit
        // invariant an external verifier checks to confirm the multiset
        // binding is correct.
        let mut entries = vec![
            sample_entry(0xdd),
            sample_entry(0xaa),
            sample_entry(0xbb),
            sample_entry(0xaa),
            sample_entry(0xcc),
            sample_entry(0xbb),
            sample_entry(0xaa),
        ];
        assign_input_ordinals(&mut entries);
        assign_occurrence_indices(&mut entries);
        sort_entries(&mut entries);

        let mut i = 0;
        while i < entries.len() {
            let group_digest = entries[i].document_id;
            let group_start = i;
            while i < entries.len() && entries[i].document_id == group_digest {
                i += 1;
            }
            let group = &entries[group_start..i];
            // occurrence_index should be 0, 1, 2, ... within the group.
            for (rank, entry) in group.iter().enumerate() {
                assert_eq!(
                    entry.occurrence_index as usize, rank,
                    "occurrence_index mismatch at group {:02x}, rank {rank}",
                    group_digest[0]
                );
            }
            // input_ordinal within the group should also be monotonically
            // increasing (the auditor's independent recompute).
            let ordinals: Vec<u64> = group.iter().map(|e| e.input_ordinal).collect();
            let mut sorted = ordinals.clone();
            sorted.sort();
            assert_eq!(
                ordinals, sorted,
                "input_ordinal not monotone in group {:02x}",
                group_digest[0]
            );
        }
    }
}
