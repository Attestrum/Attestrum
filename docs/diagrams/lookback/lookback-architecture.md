---
title: "Lookback public fuzzy-search demo — end-to-end architecture"
models: "crates/attestrum-fingerprint/src/lib.rs, crates/attestrum-manifest/src/lib.rs, crates/attestrum-fingerprint-registry/src/lib.rs, crates/attestrum-publish/src/lib.rs"
source_of_truth: diagram
last_verified: 8d49acc 2026-06-06
diagram_type: flowchart
---

# Lookback — end-to-end architecture (planning)

**Source of truth: `diagram`** — this is a forward-looking design for the Lookback demo (the public corpus fuzzy-search "shop window"). It flips to `source_of_truth: code` per lane as each phase lands. The `models:` files are the crates each lane builds on; the registry crate is still a stub (Phase B is its first real implementation).

A visitor pastes a paragraph on attestrum.com and learns whether it is inside a **real, Attestrum-sealed** public corpus (WikiText‑103), with a "verify it yourself" link to a `cosign`-checkable provenance bundle. Results are **discovery-grade, never proof-grade** (CLAUDE.md §5 / roadmap §21.3) — that wall is structural, not just copy.

The four lanes map to the phase plan:

- **Phase A — seal** the whole corpus → signed, HF-published provenance bundle.
- **Phase B — index** the sealed passages' MinHash signatures into an LSH structure for sub-linear lookup (`attestrum-fingerprint-registry`).
- **Phase C — `/search` backend** that fingerprints the visitor's paste and queries the index.
- **Phase D — website page** (primary completion) that renders results + the verify link.

The matching engine in `attestrum-prove` is deliberately **not** on the query hot path: it does a linear O(N) scan and re-fingerprints every leaf from CAS per fuzzy query — minutes at ~1M leaves. The demo's hot path uses only `attestrum_fingerprint::fingerprint_text` (to fingerprint the paste) plus the Phase‑B LSH index.

```mermaid
flowchart TD
  subgraph A["Phase A — seal + sign + publish"]
    WT["wikitext-103-raw-v1<br/>(whole train split)"] --> SEAL["seal generator<br/>passages &rarr; attestrum build"]
    SEAL --> MAN["manifest.parquet<br/>+ merkle.root"]
    MAN --> SIGN["sign in CI<br/>(Attestrum GHA keyless OIDC)"]
    SIGN --> BUNDLE["bundle.sigstore.json"]
    BUNDLE --> HF["Hugging Face<br/>Attestrum/wikitext-103-sealed"]
  end

  subgraph B["Phase B — fingerprint index"]
    IDX["MinHash-LSH index<br/>(attestrum-fingerprint-registry)"]
  end

  subgraph CD["Phase C/D — query path"]
    VISITOR["visitor pastes a paragraph<br/>(attestrum.com page — Phase D)"] --> API["POST /search backend<br/>(Phase C)"]
    API --> FP["fingerprint_text<br/>&rarr; MinHash-128"]
    FP --> QUERY["LSH query<br/>&rarr; top-k passages"]
    QUERY --> LOOKUP["resolve source article<br/>(manifest source_url backref)"]
    LOOKUP --> RESULT["discovery-grade result<br/>confidence, snippet, article"]
    RESULT --> VERIFY["verify-it-yourself link<br/>(cosign, no Attestrum)"]
  end

  MAN -. passage fingerprints .-> IDX
  IDX -. serves matches .-> QUERY
  MAN -. passage metadata .-> LOOKUP
  VERIFY -. points to .-> HF

  classDef seal fill:#1f3a5f,stroke:#4a90d9,color:#fff
  classDef index fill:#3a2f5f,stroke:#9a7ad9,color:#fff
  classDef query fill:#1f5f3a,stroke:#3ec072,color:#fff
  class WT,SEAL,MAN,SIGN,BUNDLE,HF seal
  class IDX index
  class VISITOR,API,FP,QUERY,LOOKUP,RESULT,VERIFY query
```

**Discovery-grade wall:** the path from a fuzzy LSH hit to a displayed "match" never asserts proof-grade inclusion. A proof-grade claim would require `attestrum prove` emitting a signed inclusion proof against the corpus root — out of scope for the demo, which answers "likely present" with a confidence score and points the visitor at the corpus's own verifiable bundle.
