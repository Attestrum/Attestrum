---
title: "attestrum plan and attestrum merge deterministic sharding for sub-corpus builds"
models: "crates/attestrum-cli/src/commands/plan.rs, crates/attestrum-cli/src/commands/merge.rs"
source_of_truth: code
last_verified: 4226bba 2026-05-30
diagram_type: flowchart
---

# attestrum plan and attestrum merge — deterministic sharding

Source of truth: `code` (Sprint 3 E7 implementation). Minimal sharding for v1 (founder-approved at Sprint 3 scope confirmation): correctness, not large-scale stress. The load-bearing contract is **Merkle-root equality**: building the unsharded corpus and building-then-merging the sharded variant produce the same final Merkle root. The merged `manifest.parquet` BYTES additionally equal the unsharded variant only when no cross-shard duplicate digests exist AND the shard-concat order happens to align with original input order — see the round-trip test obligation below for the exact assertion.

**Deterministic shard assignment**: `shard_id = (first 8 bytes of BLAKE3(source_url.as_bytes()), interpreted as little-endian u64) mod N`. The hash-mod-N scheme is uniform across `N` shards under normal URL distributions, depends only on the `source_url` string (not file content, not entry order), and re-running `attestrum plan` with the same `--corpus` and `--shards` produces byte-identical shard files. Duplicates of the same `source_url` always co-locate to the same shard so the multiset count is preserved within a shard. Empty shards are skipped (no `shard-NNNN.toml` emitted) — keeps the on-disk layout tidy and the merge step doesn't need to handle empty inputs.

**Merge contract**: `attestrum merge --inputs shard-*.parquet --out merged.parquet` reads each shard's already-sealed Parquet (lex-sorted input order for determinism), concatenates the rows, re-runs `assign_input_ordinals` GLOBALLY (so each merged row has a unique 0..N input_ordinal in concat order — the E2.5 audit invariant continues to hold within the merged file), re-runs `assign_occurrence_indices` GLOBALLY (so cross-shard identical digests get a unified occurrence count), re-runs `sort_entries` by `(document_id, occurrence_index)`, then writes one merged Parquet. The merged Merkle root equals the root we would have computed had we built the corpus unsharded.

**Why merged bytes ≠ unsharded bytes (in the general shards > 1 case)**: the unsharded build's `input_ordinal` column reflects the original input order (the position in the source corpus.toml); the merged build's `input_ordinal` reflects shard-concat order, which is `(shard_id ascending, within-shard canonical sort)`. These only coincide when entries happen to be listed in shard_id-ascending order in the original corpus.toml — a synthetic property. The Merkle root computes only over sorted digests, so it's invariant under any permutation that preserves the multiset; byte-identity of the full Parquet is a stricter property that requires `input_ordinal` agreement.

```mermaid
flowchart TD
  IN[corpus.toml with Vec CorpusEntry] --> PLAN[attestrum plan --corpus corpus.toml --shards N --out shards dir]
  PLAN --> HASH[for each entry: shard_id = BLAKE3 source_url first 8 bytes LE u64 mod N]
  HASH --> EMIT[write shards dir shard 0000.toml ... shard N minus 1.toml empty shards skipped]
  EMIT --> WORK0[worker 0: attestrum build shard 0000.toml --workspace shard 0000]
  EMIT --> WORK1[worker 1: attestrum build shard 0001.toml --workspace shard 0001]
  EMIT --> WORKN[worker N: attestrum build shard N minus 1.toml --workspace shard N minus 1]
  WORK0 --> SHARD0[shard 0000 .attestrum manifests manifest.parquet]
  WORK1 --> SHARD1[shard 0001 .attestrum manifests manifest.parquet]
  WORKN --> SHARDN[shard N minus 1 .attestrum manifests manifest.parquet]
  SHARD0 --> MERGE[attestrum merge --inputs ... --out merged.parquet]
  SHARD1 --> MERGE
  SHARDN --> MERGE
  MERGE --> READ[for each input lex-sorted: read_manifest returns Vec ManifestEntry]
  READ --> CONCAT[concatenate all Vec ManifestEntry into single Vec]
  CONCAT --> REIO[assign_input_ordinals global pass]
  REIO --> REASS[assign_occurrence_indices global pass]
  REASS --> RESORT[sort_entries by document_id then occurrence_index]
  RESORT --> WRITE[write_manifest merged.parquet]
  WRITE --> OUT[merged.parquet plus optional merged merkle root via attestrum inspect]
```

**Public surface** (in `crates/attestrum-cli/src/commands/`):

- `pub mod plan` with `pub struct Args { corpus, shards, out }`, `pub enum PlanError { InvalidShardCount, CorpusMissing, CorpusRead, CorpusParse, EntryMissingSourceUrl, OutputDir, EmitShard, Serialize(#[from] toml::ser::Error) }`, `pub fn run(args) -> u8`, `pub fn shard_id(source_url: &str, shards: u32) -> u32`.
- `pub mod merge` with `pub struct Args { inputs, out }`, `pub enum MergeError { NoInputs, InputRead, OutputDir, Write(#[from] attestrum_core::AttestrumError) }`, `pub fn run(args) -> u8`.

**Test obligations** (per CLAUDE.md §7.1 flowchart → integration-edges test, satisfied at Sprint 3 E7 in `crates/attestrum-cli/tests/sharding.rs`):

- `plan_shards_1_is_noop_single_shard_file` — `--shards 1` writes a single `shard-0000.toml` whose `[[entry]]` count equals the input.
- `plan_shards_equal_entry_count_one_per_shard_when_unique_urls` — `--shards N` where N == entry count, distinct source_urls: emits 1..N shard files (empty shards skipped per hash distribution), the union of all shards' entries equals the input, and every emitted shard has ≥ 1 entry.
- `plan_duplicate_source_urls_colocate_in_same_shard` — 5 entries with identical `source_url` always land in the same single shard file (preserves multiset binding pre-build).
- `plan_re_run_produces_identical_shard_files` — running `attestrum plan` twice with the same args (against two different output dirs) produces byte-identical shard files in each.
- `merge_round_trip_matches_unsharded_build` — build corpus directly → root A; plan + build-each-shard + merge → root B; assert `root A == root B` AND assert the sorted leaf-set (sorted Vec<[u8; 32]> of document_ids) is identical between the two manifests. Note: byte-equality of `manifest.parquet` is NOT asserted because merged manifests' `input_ordinal` reflects merge-concat order rather than original input order; the deterministic-sharding contract is root equality + leaf-set equality, not full byte-identity.
- `merge_with_overlapping_digests_across_shards_globally_reassigns_occurrence_indices` — two single-entry shards each containing byte-identical content but distinct source_urls produce a merged manifest with two rows sharing `document_id` and carrying `occurrence_index` 0 and 1 globally (not 0 and 0 from their per-shard assignment).

**Out of scope for E7** (deferred to later sprints):

- Distributed coordinator (running shard builds on different machines and merging via S3) — Sprint 4 with the S3 backend.
- Large-scale stress test (>100k entries × >100 shards) — Sprint 5 with the 1 GB Common-Pile-mini benchmark; v1 acceptance is correctness on small synthetic corpora.
- Inter-shard CAS sharing (shard builds today write to per-shard `.attestrum/cas/`; a future variant could share a global `.attestrum/cas/` and dedup-via-CAS) — v1.1.
- Restoring full byte-identity between merged and unsharded — would require canonicalizing `build_corpus`'s `input_ordinal` assignment by source_url instead of by input order (a PROTECTED-system-change with SCHEMA_VERSION bump). Defer until a real user use case demands it; root equality is sufficient for the Merkle-based attestations Attestrum emits.
