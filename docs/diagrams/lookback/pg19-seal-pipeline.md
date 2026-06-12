---
title: "Lookback Tier-1 — deepmind-pg19 seal generator pipeline"
models: "crates/attestrum-pipeline/examples/seal-pg19.rs, crates/attestrum-pipeline/src/lib.rs"
source_of_truth: code
last_verified: 918d0e5 2026-06-12
diagram_type: flowchart
---

# Lookback Tier-1 — deepmind-pg19 seal generator

**Source of truth: `code`** — the seal generator
`crates/attestrum-pipeline/examples/seal-pg19.rs` (with its file-walking core
`examples/pg19/seal.rs`) has landed; this diagram is now a derived view of it. Third Tier-1 reference bundle after WikiText-103
(`wikitext-seal-pipeline.md`) and dolly-15k (`dolly-seal-pipeline.md`), and the
first **large** rung: 28,752 plain-text books, ~11.5 GB — the corpus is
downloaded and sealed in CI, never on a laptop and never committed.

Three decisions shape the pipeline (deviations from dolly are deliberate):

1. **One book file = one leaf, sealed as its exact bytes.** PG-19 ships as
   28,752 individual UTF-8 `.txt` files (train 28,602 / validation 50 / test
   100), one complete pre-1919 Project Gutenberg book each, max ~4.5 MB. There
   is **no rendering step at all** — the sealed bytes are the file bytes,
   identity-mapped, so every leaf digest is independently checkable against the
   upstream file with `b3sum`. All three splits are sealed: the corpus is the
   whole dataset. `metadata.csv`, the README, and the loader script are an
   index, not corpus content — pinned in `docs/lookback/pg19-corpus-source.md`,
   not sealed.
2. **Entries are `ContentSource::Path`, not `Bytes`.** At 11.5 GB the corpus
   must never sit in RAM. `build_corpus`'s Path branch reads each file once per
   worker, so peak memory is O(workers x largest file) — a few tens of MB —
   inside the free-runner 7 GB envelope.
3. **Determinism is preserved.** Files are enumerated from the input dir's
   `train/`, `validation/`, `test/` subdirs and sorted lexicographically by
   relative path; `build_corpus` stamps `input_ordinal` then sorts canonically,
   so the same file tree seals to a byte-identical `manifest.parquet` + Merkle
   root (the `sprint-3-corpus` determinism contract). The `source_uri` backref
   is `pg19://<relative-path>` (e.g. `pg19://train/10005.txt`).

```mermaid
flowchart TD
  subgraph SRC["Upstream (pinned in pg19-corpus-source.md)"]
    LISTS["HF deepmind/pg19 @ 4d28bd7<br/>data/{train,validation,test}_files.txt<br/>(SHA-256 pinned, 28,752 paths)"]
    GCS["GCS deepmind-gutenberg bucket<br/>28,752 plain-text book files<br/>~11.5 GB, max file ~4.5 MB"]
  end

  subgraph CI["CI runner (download step, workflow shell)"]
    DL["aria2c bulk download<br/>asserts: count per split,<br/>no zero-byte files"]
  end

  subgraph GEN["examples/seal-pg19.rs"]
    WALK["enumerate {train,validation,test}/*.txt<br/>sort lexicographically by relative path"]
    ENTRY["CorpusEntry { content: Path(file),<br/>source_uri: pg19://train/10005.txt,<br/>modality: Text, license: Apache-2.0 }"]
    WALK --> ENTRY
  end

  subgraph PIPE["attestrum_pipeline::build_corpus"]
    BUILD["hash (BLAKE3 + SHA-256, streamed)<br/>+ CAS put + sort + Merkle"]
    OUT["manifest.parquet (28,752 rows)<br/>+ merkle.root"]
    CAS["CAS (.attestrum/cas) ~11.5 GB<br/>exact book bytes — stays on runner"]
    BUILD --> OUT
    BUILD --> CAS
  end

  LISTS --> DL
  GCS --> DL
  DL --> WALK
  ENTRY --> BUILD

  DET["determinism test:<br/>fixture file tree sealed twice<br/>-> identical manifest + root"]
  OUT -.checked by.-> DET

  classDef in fill:#5f4a1f,stroke:#e0a52e,color:#fff
  classDef ci fill:#5f1f3a,stroke:#d94a90,color:#fff
  classDef gen fill:#1f3a5f,stroke:#4a90d9,color:#fff
  classDef pipe fill:#1f5f3a,stroke:#3ec072,color:#fff
  classDef test fill:#3a2f5f,stroke:#9a7ad9,color:#fff
  class LISTS,GCS in
  class DL ci
  class WALK,ENTRY gen
  class BUILD,OUT,CAS pipe
  class DET test
```

**Why exact bytes, not a render:** dolly's unit of meaning was a multi-column
row that needed rendering into prose; PG-19's unit is already a single natural
file. Sealing the file bytes untouched is the strongest possible provenance
claim — no transform to disclose, no render contract to drift — and it keeps
the published manifest a direct per-file digest list of the upstream corpus.
The PROTECTED `attestrum-fingerprint` normalization (CLAUDE.md §4) is untouched,
as in both prior rungs.
