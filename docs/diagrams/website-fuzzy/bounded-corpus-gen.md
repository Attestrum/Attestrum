---
title: "C3 bounded fuzzy-corpus generator — showcase passages → kernel → fuzzy-index.json"
models: "tools/fuzzy-web-gen/src/lib.rs, tools/fuzzy-web-gen/src/main.rs, tests/fixtures/showcase-passages/display.json, tests/fixtures/fuzzy-web/fuzzy-index.json, normalize_text"
source_of_truth: code
last_verified: e71552c 2026-06-12
diagram_type: flowchart
---

# C3 — bounded fuzzy-corpus generator

Source of truth: **`code`**. `tools/fuzzy-web-gen` is authoritative; this diagram is the derived view.

**What it produces.** The attestrum.com near-match demo matches a visitor's pasted text against a
*bounded* corpus shipped to the browser. C3 generates that corpus artifact — `fuzzy-index.json` — from the
5 curated showcase passages (founder-scoped; real LSH-neighbors over the full 822k corpus stay deferred).

**One kernel, end to end.** `fuzzy-web-gen` computes each passage's 128-permutation MinHash with
`attestrum-text-minhash` (`normalize_text` → `minhash::compute`) — the **same PROTECTED kernel** the C2
wasm compiles from and that `attestrum index build` / `attestrum prove` use. So the corpus signatures in
`fuzzy-index.json` are byte-identical to what the browser's wasm recomputes (C2's cross-check gate proves
wasm == kernel) and to what the CLI would match against. The demo is a faithful client-side mirror of the
production fingerprint path, not a re-implementation.

**Why a single JSON, not a binary index.** At 5 leaves the artifact is ~22 KB; the browser fetches it
wholesale and brute-forces exact Jaccard against the 5 signatures — the exhaustive **recall oracle**,
byte-identical in result to the production LSH candidate path, with no banding machinery. The scalable
`.idx`→binary band-directory + Range-fetch re-encoder is deferred to land with the full corpus (when the
format gets its second, real use). `params` mirrors the kernel/threshold constants
(`sigWidth 128`, `bands 32`, `rows 4`, `jaccardThresholdPpm 850000`, `ngram 5`) so the C4 glue stays in
sync without hardcoding them.

**Reproducible + tracked.** `tools/fuzzy-web-gen/tests/reproducibility.rs` regenerates the artifact and
byte-compares it to the committed golden, and asserts every leaf signature equals the kernel and that
`snippet` is the exact byte-source of `sig` (so C4's in-page conformance check — `wasm(normalize(snippet))
== sig` — holds). This fills the gap that the demo's browser artifacts previously had no tracked generator.
The committed JSON is the source of truth; C4 copies it to the landing site as an intentional public asset.

```mermaid
flowchart TB
  classDef protected fill:#7a1f1f,stroke:#c63737,color:#fff
  classDef tool fill:#1f6f3f,stroke:#3ec072,color:#fff
  classDef data fill:#1a3a6f,stroke:#3a8ed7,color:#fff
  classDef gate fill:#8a5a00,stroke:#e0a52e,color:#fff
  classDef future fill:#3a3a3a,stroke:#666,color:#aaa
  %% shipped + revised-this-revision highlight (green fill, amber thick border)
  classDef shipped fill:#1f6f3f,stroke:#e0a52e,stroke-width:4px,color:#fff

  subgraph inputs["curated inputs (tests/fixtures/showcase-passages/)"]
    P["passage-01..05.txt<br/>(byte-exact sealed leaves)"]
    D["display.json<br/>(title · url · passageId, row order)"]
  end
  class P,D data

  subgraph kernel["attestrum-text-minhash (PROTECTED §4)"]
    K["normalize_text → minhash::compute<br/>128 perms"]
  end
  class K protected

  GEN["fuzzy-web-gen<br/>build_fuzzy_index → to_json"]
  class GEN tool

  OUT["tests/fixtures/fuzzy-web/fuzzy-index.json<br/>5 leaves: row · title · url · snippet · sig[128]"]
  class OUT data

  REPRO["tests/reproducibility.rs<br/>regen == golden · sig == kernel · snippet == sig source"]
  class REPRO gate

  BROWSER["attestrum.com near-match demo — LIVE<br/>(C4 — wasm query sig + brute-force Jaccard ≥ 0.85)"]
  class BROWSER shipped

  P --> GEN
  D --> GEN
  P --> K
  K --> GEN
  GEN --> OUT
  OUT --> REPRO
  K --> REPRO
  OUT --> BROWSER
```

🟧 revised this revision: the browser near-match demo is now **LIVE** on attestrum.com — C4 shipped 2026-06-06. The browser lazy-loads `corpus-index/attestrum_fingerprint_wasm.wasm` (the C2 kernel) + `corpus-index/fuzzy-index.json` (this artifact), runs an in-page conformance check (wasm sig of a known passage == its shipped sig), then on an exact miss computes the pasted text's MinHash and brute-forces Jaccard ≥ 0.85 against the 5 leaves.
