---
title: "takedown flow with public witness"
models: "crates/attestrum-ledger/src/lib.rs::append_takedown, crates/attestrum-ledger/src/witness.rs"
source_of_truth: diagram
last_verified: 7db9838 2026-06-12
diagram_type: flowchart
---

# Takedown + public witness

Source of truth flips to `code` at end of Sprint 6 when `RekorWitness` / `HubWitness` land. The Rekor v2 path uses the public-good instance whose URL is distributed via TUF (the Sigstore docs warn against hardcoding it: "We strongly advise against hardcoding this URL into any pipelines that cannot be easily updated"). The hub-witness path is a fallback we operate ourselves on the Hub when Rekor v2 is unavailable, contractually equivalent to a tiled append-only log.

```mermaid
flowchart TD
  R[takedown request<br/>rightsholder + doc hash + reason] --> V[verify standing<br/>attestrum-ledger]
  V --> L[append takedown leaf<br/>local append-only log]
  L --> W{witness mode?}
  W -->|local only| NV[new corpus version<br/>v_n+1]
  W -->|rekor| RK[submit leaf to Rekor v2<br/>predicate: takedown/v0.1]
  W -->|hub-witness| HB[append leaf to<br/>huggingface.co/datasets/&lt;org&gt;/&lt;dataset&gt;-witness/log.jsonl]
  RK --> NV
  HB --> NV
  NV --> CH[cryptographic chain<br/>v_n+1.prev_root = v_n.merkle_root]
  CH --> SIGN[sign new manifest<br/>training-corpus/v0.1 predicate]
  SIGN --> PUB[republish to HF dataset repo<br/>attestrum publish]
  PUB --> NOTIFY[notify downstream consumers<br/>via Hub webhook]
```
