---
title: "Lookback Phase A — WikiText-103 seal generator pipeline"
models: "crates/attestrum-pipeline/examples/seal-wikitext.rs, crates/attestrum-pipeline/src/lib.rs"
source_of_truth: code
last_verified: dae1a12 2026-06-07
diagram_type: flowchart
---

# Lookback Phase A — WikiText-103 seal generator

**Source of truth: `code`** — the programmatic seal generator
`crates/attestrum-pipeline/examples/seal-wikitext.rs` (with its Parquet-reading core
`examples/wikitext/seal.rs`) has landed; this diagram is now a derived view of it.
It follows the deterministic `sprint-3-corpus.rs` pattern but reads the real corpus
from the Hugging Face parquet `text` column instead of synthesising bytes in memory.

Three decisions shape the pipeline:

1. **Detokenization is mandatory and lives here, in segmentation — never in the PROTECTED
   `attestrum-fingerprint` normalization** (CLAUDE.md §4). WikiText-103-raw ships
   moses-tokenized (` @-@ `, spaced punctuation); a pasted natural-English paragraph
   matches a raw passage at ~0.32 Jaccard but ~1.00 after the passage is detokenized.
   The CAS therefore stores detokenized bytes, so Phase B re-fingerprints natural English.
2. **Each passage is one leaf.** A passage is one non-empty, non-header line of the
   `text` column; the single-`=` article-title line becomes the `source_uri` backref
   `wikipedia://<slug>#p<N>`; a min-word floor drops tiny fragments.
3. **Determinism is preserved.** Shards are read in fixed filename order, rows in file
   order; `build_corpus` stamps `input_ordinal` then sorts canonically, so the same
   input yields a byte-identical `manifest.parquet` + Merkle root (the
   `sprint-3-corpus` determinism contract, extended to the real corpus).

```mermaid
flowchart TD
  subgraph IN["Input (local, gitignored)"]
    SH["wikitext-103-raw-v1<br/>train parquet shards<br/>(fixed filename order)"]
  end

  subgraph GEN["examples/seal-wikitext.rs"]
    READ["read 'text' column<br/>(arrow + parquet dev-deps)"]
    SEG["segment lines<br/>skip header ^=+ .. =+$<br/>capture single-= title<br/>min-word floor"]
    DETOK["detokenize passage<br/>@-@ -> -, despace punctuation,<br/>rejoin contractions"]
    ENTRY["CorpusEntry { content: Bytes(detok),<br/>source_uri: wikipedia://slug#pN,<br/>modality: Text }"]
    READ --> SEG --> DETOK --> ENTRY
  end

  subgraph PIPE["attestrum_pipeline::build_corpus"]
    BUILD["hash (BLAKE3 + SHA-256)<br/>+ CAS put + sort + Merkle"]
    OUT["manifest.parquet<br/>+ merkle.root"]
    CAS["CAS (.attestrum/cas)<br/>detokenized bytes — stays local"]
    BUILD --> OUT
    BUILD --> CAS
  end

  SH --> READ
  ENTRY --> BUILD

  DET["determinism test:<br/>fixed subset sealed twice<br/>-> identical manifest + root"]
  OUT -.checked by.-> DET

  classDef in fill:#5f4a1f,stroke:#e0a52e,color:#fff
  classDef gen fill:#1f3a5f,stroke:#4a90d9,color:#fff
  classDef pipe fill:#1f5f3a,stroke:#3ec072,color:#fff
  classDef test fill:#3a2f5f,stroke:#9a7ad9,color:#fff
  class SH in
  class READ,SEG,DETOK,ENTRY gen
  class BUILD,OUT,CAS pipe
  class DET test
```

**Why detok in segmentation, not fingerprinting:** the `attestrum-fingerprint`
normalization (NFC → lowercase → whitespace-collapse) is a PROTECTED, version-locked
invariant (CLAUDE.md §4) shared by every emitted inclusion proof; changing it would
invalidate prior proofs. Moses detokenization is corpus-preparation, not fingerprint
normalization — it transforms what bytes get sealed, upstream of and independent from the
fingerprint path. Sealing the detokenized text is honest as long as the published dataset
card states the corpus was detokenized to natural English (recorded at publish time).
