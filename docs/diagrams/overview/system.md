---
title: "Attestrum system overview — inputs to outputs"
models: "crates/attestrum-core, crates/attestrum-signals, crates/attestrum-cas, crates/attestrum-merkle, crates/attestrum-manifest, crates/attestrum-fingerprint, crates/attestrum-ledger, crates/attestrum-pipeline, crates/attestrum-attest, crates/attestrum-emit, crates/attestrum-prove, crates/attestrum-publish"
source_of_truth: diagram
last_verified: a8e49bd 2026-05-27
diagram_type: flowchart
---

# System overview

Highest-level system map. The Mermaid is the contract; downstream code must implement every edge. Verification strategy: the Sprint 6 end-to-end demo recording must visibly traverse each path from at least one Input to at least one Output, and the demo transcript is checked in alongside this file. `source_of_truth` flips to `code` at the end of Sprint 6.

```mermaid
flowchart LR
  subgraph Inputs
    A1[corpus.toml]
    A2[opt-out signals<br/>robots.txt · ai.txt · TDMRep · AIPref<br/>IPTC-PLUS · C2PA · RSL · Liccium · Cloudflare]
    A3[raw documents<br/>local FS · S3 · HF Hub]
    A4[rightsholder fingerprints<br/>optional]
  end

  subgraph AttestrumCompiler["Attestrum compiler (crates)"]
    C1[attestrum-core]
    C2[attestrum-signals]
    C3[attestrum-cas]
    C4[attestrum-merkle]
    C5[attestrum-manifest]
    C6[attestrum-fingerprint]
    C7[attestrum-attest]
    C8[attestrum-emit]
    C9[attestrum-publish]
    C10[attestrum-prove]
  end

  subgraph Outputs
    O1[manifest.parquet]
    O2[merkle.root]
    O3[Sigstore bundle<br/>training-corpus/v0.1]
    O4[Article 53 PDF + JSON]
    O5[Croissant JSON-LD]
    O6[CycloneDX ML-BOM]
    O7[HF dataset card<br/>README.md + YAML]
    O8[public verification page<br/>verify.html]
  end

  subgraph Persistent["Persistent state"]
    L1[(takedown ledger<br/>append-only)]
  end

  A1 --> C1
  A2 --> C2
  A3 --> C3
  A4 --> C6
  C1 --> C5
  C2 --> C5
  C3 --> C4
  C3 --> C6
  C4 --> C5
  C5 --> C7
  C6 --> C7
  C7 --> C8
  C7 --> C9
  C7 --> C10
  C5 --> O1
  C4 --> O2
  C7 --> O3
  C8 --> O4
  C8 --> O5
  C8 --> O6
  C9 --> O7
  C9 --> O8
  L1 -.witness.-> C7
  C7 -.append.-> L1
```
