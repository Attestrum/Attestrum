---
title: "attestrum build pipeline — happy path"
models: "crates/attestrum-pipeline/src/lib.rs::run, crates/attestrum-cas/src/store.rs, crates/attestrum-signals, crates/attestrum-merkle, crates/attestrum-manifest"
source_of_truth: diagram
last_verified: bootstrap 2026-05-24
diagram_type: flowchart
---

# attestrum build — happy path

Source of truth flips to `code` at end of Sprint 3 when `attestrum-pipeline::run` lands. Integration test `tests/pipeline_happy_path.rs` exercises every edge with a 10-MB fixture corpus checked in under `tests/fixtures/mini-pile/`.

```mermaid
flowchart TD
  S[start: attestrum build] --> L[load corpus.toml]
  L --> P[shard plan<br/>attestrum-plan]
  P --> F[parallel fetch<br/>rayon worker pool]
  F --> SP[signal parse<br/>attestrum-signals]
  SP --> H[stream hash<br/>BLAKE3 + SHA-256]
  H --> CW[CAS write<br/>.attestrum/cas/blake3/aa/bb/...]
  CW --> RD{ruleset decision<br/>strict / audit-only / permissive}
  RD -->|include| MR[manifest row append]
  RD -->|exclude with reason| MX[exclusion row append]
  MR --> SE[seal Parquet shard]
  MX --> SE
  SE --> MK[Merkle root<br/>RFC 6962 binary]
  MK --> CS[corpus summary<br/>counts · sizes · signal coverage]
  CS --> E[exit 0]
```
