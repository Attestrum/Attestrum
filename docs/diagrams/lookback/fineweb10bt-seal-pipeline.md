---
title: "Lookback — fineweb-edu sample-10BT sharded-matrix seal pipeline"
models: "crates/attestrum-pipeline/examples/seal-fineweb-edu.rs, crates/attestrum-cli/src/commands/merge.rs"
source_of_truth: code
last_verified: 39fa850 2026-06-12
diagram_type: flowchart
---

# Lookback — fineweb-edu sample-10BT sharded-matrix seal

**Source of truth: `code`** — the seal generator
`crates/attestrum-pipeline/examples/seal-fineweb-edu.rs` (with its
parquet-reading core `examples/fineweb_edu/seal.rs`) has landed; this diagram
is now a derived view of it and of `attestrum merge`. Fourth reference
bundle after WikiText-103, dolly-15k, and PG-19, and the ladder's first
**sharded** rung: 9,672,101 web pages / 28.5 GB compressed parquet — too big
for one runner's disk, so this rung exists to prove the architecture that
scales to the 286 GB rung above it: **N matrix jobs each seal one slice;
`attestrum merge` combines the shard manifests into one canonical root.** The
corpus never moves between jobs; only manifests (~55 MB/shard) do.

Three decisions shape the pipeline:

1. **Shard = one upstream parquet file, statically pinned.** `sample/10BT/`
   ships as exactly 14 parquet files (13 × ~2.15 GB + one 540 MB tail), each
   with an upstream LFS SHA-256 (pinned in
   `docs/lookback/fineweb10bt-corpus-source.md`). The matrix enumerates the 14
   (filename, sha256) pairs — no dynamic listing, no `attestrum plan`; the
   dataset's own layout is the natural partition. Each shard job's footprint
   (~2.15 GB download + ~3.4 GB CAS) fits a stock runner.
2. **One row = one leaf; sealed bytes = the `text` column bytes exactly.** No
   transform, no added newline — the PG-19 exact-bytes philosophy applied to a
   column. Metadata columns (`url`, `dump`, scores, …) are not sealed.
   `source_uri` is the row's own `id` (a `urn:uuid` — globally unique and
   shard-invariant), so a leaf's identity does not depend on which shard sealed
   it. Per-shard `input_ordinal` restarts are fine: `merge` re-stamps ordinals
   globally over the concatenation.
3. **The merged root is the canonical root.** `attestrum merge` k-way merges
   the shard manifests (lexicographic input order) in a single streaming pass,
   stamping `input_ordinal` + `occurrence_index` globally as it emits, recomputes
   the RFC 6962 root over the sorted leaf set, prints `merkle_root: <hex>`, and
   writes `merkle.root` beside the merged manifest — byte-identical to what an
   (infeasible) unsharded build would produce (multiset invariance; proven by
   `crates/attestrum-cli/tests/sharding.rs` + `tests/merge_byte_identity.rs` and
   re-proven through this example's split-vs-whole test).

```mermaid
flowchart TD
  subgraph SRC["Upstream (pinned in fineweb10bt-corpus-source.md)"]
    HF["HF HuggingFaceFW/fineweb-edu @ 87f0914<br/>sample/10BT/ — 14 parquet files, 28.5 GB<br/>per-file LFS SHA-256 pinned, 9,672,101 rows"]
  end

  subgraph MATRIX["CI seal job — matrix of 14 shards, one per parquet file"]
    DL["inline disk cleanup<br/>curl own shard NNN_00000.parquet<br/>assert pinned SHA-256"]
    subgraph GEN["examples/seal-fineweb-edu.rs"]
      READ["read parquet rows:<br/>text, id, language columns"]
      ENTRY["CorpusEntry { content: Bytes(text bytes),<br/>source_uri: row id (urn:uuid),<br/>modality: Text, license: ODC-By-1.0 }"]
      READ --> ENTRY
    end
    BUILD["attestrum_pipeline::build_corpus<br/>hash + CAS + sort + Merkle (per shard)"]
    ART["upload artifact shard-NN:<br/>manifest.parquet (~55 MB)<br/>CAS stays on the runner, discarded"]
    DL --> READ
    ENTRY --> BUILD
    BUILD --> ART
  end

  subgraph MERGE["CI merge job (needs: all 14 seal jobs)"]
    GATHER["download 14 shard artifacts"]
    MRG["attestrum merge --out manifest.parquet<br/>streaming k-way merge, lexicographic input order<br/>stamp input_ordinal + occurrence_index globally"]
    ROOT["merged manifest.parquet (9,672,101 rows)<br/>+ merkle_root: hex line + merkle.root file"]
    TRIPLE["mode=capture: record triple<br/>mode=assert: root + manifest SHA-256<br/>+ leaves == 9,672,101 must reproduce"]
    GATHER --> MRG --> ROOT --> TRIPLE
  end

  HF --> DL
  ART --> GATHER

  DET["tests: fixture round-trip, seal-twice determinism,<br/>split-vs-whole — all rows at once == two halves merged,<br/>identical root (the sharding contract through these leaves)"]
  ROOT -.checked by.-> DET

  classDef in fill:#5f4a1f,stroke:#e0a52e,color:#fff
  classDef ci fill:#5f1f3a,stroke:#d94a90,color:#fff
  classDef gen fill:#1f3a5f,stroke:#4a90d9,color:#fff
  classDef pipe fill:#1f5f3a,stroke:#3ec072,color:#fff
  classDef test fill:#3a2f5f,stroke:#9a7ad9,color:#fff
  class HF in
  class DL,ART,GATHER,TRIPLE ci
  class READ,ENTRY gen
  class BUILD,MRG,ROOT pipe
  class DET test
```

**Why `Bytes`, not `Path`:** PG-19's unit was a file on disk, streamed via
`ContentSource::Path`. Here the unit is a parquet row — the text exists only
after column decode, so each row's text bytes are passed as
`ContentSource::Bytes`. Rows are sealed in file order within a shard
(parquet row order is deterministic), and `build_corpus`'s canonical sort makes
the per-shard manifest deterministic regardless. Peak RSS per shard is bounded
by the parquet reader's batch size, not the corpus.

**Why the merge job measured RSS (and what it drove):** the capture run
measured the original load-everything merge at **8.7 GiB peak for 9.67M rows**
(~940 B/row) via `/usr/bin/time -v` — the calibration datum that showed the
~100M-row 286 GB rung would need ~90 GiB, infeasible on a free runner. That
measurement drove the **streaming k-way merge** (now landed — see
`sharding.md`): `merge` no longer holds all rows; peak is bounded by one Parquet
row group plus the 32-B-per-row leaf-digest vector (~4–5 GiB est. at 100M rows).
The merged output is byte-identical, so this 10BT triple still reproduces
exactly.

The PROTECTED `attestrum-fingerprint` normalization and all other §4 systems
(CLAUDE.md) are untouched, as in all prior rungs.
