---
title: "attestrum decontaminate — corpus × benchmark contamination scan over the shared MinHash kernel"
models: "crates/attestrum-decontaminate/src/lib.rs, crates/attestrum-decontaminate/src/ingest.rs, crates/attestrum-decontaminate/src/shingle.rs, crates/attestrum-decontaminate/src/detect.rs, crates/attestrum-decontaminate/src/report.rs, crates/attestrum-cli/src/commands/decontaminate.rs, normalize_text"
source_of_truth: code
last_verified: e71552c 2026-06-12
diagram_type: flowchart
---

# `attestrum decontaminate` — benchmark-contamination scan

Source of truth: **`code`**. The `attestrum-decontaminate` crate is authoritative; this diagram is the
derived view.

**What it answers.** "Did evaluation-benchmark questions leak into this corpus?" — a corpus-composition
question, the same family as inclusion/non-inclusion. v1 is a **read-only, unsigned** report:
`attestrum decontaminate <corpus...> --against <benchmark...>` emits `report.json` + `report.md`. No
signed predicate, no manifest mutation — it consumes the corpus and benchmark files and produces evidence.

**One determinism ruler — it rides the PROTECTED kernel, it does not fork it.** Normalization and the
near-duplicate signal reuse `attestrum-text-minhash` unchanged: `normalize_text` (NFC → lowercase →
whitespace-collapse) and `minhash::compute` (128-permutation, BLAKE3-keyed, 5-gram word shingles). So the
contamination near-dup basis is **byte-identical** to what `attestrum index` / `attestrum prove` use — the
same ruler everywhere. The decontaminate crate does **not** ship a second MinHash and does **not** modify
any §4 protected system; it only *consumes* the kernel read-only.

**Two additive signals layer on top.** The exact (13-gram collision) and containment signals need raw
shingle *sets*, which the kernel doesn't expose, so the crate adds a small BLAKE3 shingle-set helper
(`shingle.rs`). This is purely additive — workspace is BLAKE3-everywhere, so no `xxhash` dependency, and
nothing protected changes. Three signals decide a hit; **any one** firing flags the (doc, item) pair:

- **exact** — the doc and the benchmark item share at least one `EXACT_N`-gram (13 words) (catches
  verbatim leakage).
- **near** — MinHash Jaccard ≥ `near_threshold` (default `DEFAULT_NEAR_THRESHOLD` = 0.80) (catches
  paraphrase / light edit).
- **contained** — ≥ `DEFAULT_CONTAINMENT_THRESHOLD` (0.90) of the item's `NEAR_N`-gram (5-word) shingles
  appear in the doc (catches an answer buried in filler, where Jaccard is diluted below threshold).

Each hit carries a `SNIPPET_CHARS`-truncated (120-char) normalized snippet of the benchmark item for the
human-readable `report.md`. The crate-level constants `EXACT_N`, `NEAR_N`, `DEFAULT_NEAR_THRESHOLD`,
`DEFAULT_CONTAINMENT_THRESHOLD`, and `SNIPPET_CHARS` live in `attestrum-decontaminate`'s `lib.rs` and are
recorded verbatim in `report.json`'s `parameters` block so a reader can reproduce the scan.

**Determinism is the product (same discipline as the rest of the workspace).** rayon scans documents in
parallel, but every hit is collected and sorted (`benchmark, item_id, doc_id`) before reporting; all keyed
aggregates use `BTreeMap`; the canonical `report.json` is serialized through the workspace's single
sanctioned primitive `attestrum_attest::deterministic_json` (recursive key sort) — not a hand-rolled
serializer — so it shares one tested determinism basis with `attestrum diff` and the attestation emitters;
floats are round-6'd for stable formatting; no wall-clock unless an explicit
`--source-date-epoch`-style timestamp is supplied. Same corpus + same benchmarks in → byte-identical
`report.json` out, enforced by a double-run + committed-golden test (mirrors `attestrum-fingerprint`).

```mermaid
flowchart TB
  classDef protected fill:#7a1f1f,stroke:#c63737,color:#fff
  classDef additive  fill:#1f4d7a,stroke:#3e86c6,color:#fff
  classDef output    fill:#1f6f3f,stroke:#3ec072,color:#fff

  subgraph inputs[Inputs]
    corpus["corpus files<br/>(.jsonl / .parquet)"]
    benches["benchmark files<br/>--against (.jsonl / .parquet)"]
  end

  corpus --> ingestC["ingest.rs<br/>read_corpus → Vec&lt;Doc{id,text}&gt;"]
  benches --> ingestB["ingest.rs<br/>read_corpus → benchmark items"]

  %% Benchmark-side preprocessing builds the inverted indexes.
  ingestB --> normB["normalize_text"]
  normB --> bExact["13-gram shingle set"]
  normB --> bNear["5-gram shingle set<br/>(containment denominator)"]
  normB --> bSig["minhash::compute → Signature"]
  bExact --> exIdx["exact inverted index<br/>13-gram hash → item refs"]
  bNear --> nrIdx["near inverted index<br/>5-gram hash → item refs"]

  %% Document-side scan (rayon-parallel, order-stable).
  ingestC --> normD["normalize_text"]
  normD --> dExact["13-gram shingles"]
  normD --> dNear["5-gram shingles"]
  normD --> dSig["minhash::compute → Signature"]

  dExact --> cand["candidate (doc,item) pairs<br/>via index lookups"]
  dNear --> cand
  exIdx --> cand
  nrIdx --> cand

  cand --> sigExact{"shared 13-gram > 0?"}
  cand --> sigNear{"Jaccard(dSig,bSig)<br/>≥ near_threshold?"}
  cand --> sigCont{"item-shingle containment<br/>≥ containment_threshold?"}
  dSig --> sigNear
  bSig --> sigNear
  bNear --> sigCont

  sigExact -->|exact| hit["Hit { flags, shared_exact,<br/>jaccard, containment }"]
  sigNear -->|near| hit
  sigCont -->|contained| hit

  hit --> sort["collect + sort<br/>(benchmark, item_id, doc_id)"]
  sort --> aggregate["report.rs<br/>per-benchmark sections<br/>(BTreeMap) + hit list"]
  aggregate --> rjson["report.json<br/>(deterministic_json,<br/>trailing newline)"]
  aggregate --> rmd["report.md<br/>(human summary)"]

  class normB,bNear,bSig,normD,dNear,dSig protected
  class ingestC,ingestB,bExact,dExact,exIdx,nrIdx,shingle additive
  class rjson,rmd output

  %% Legend: 🟥 reused PROTECTED kernel (attestrum-text-minhash, unchanged) ·
  %% 🟦 new additive code in attestrum-decontaminate · 🟩 emitted artifacts
```

**Scope fence (v1).** Benchmarks are supplied as **local files** via `--against` — no network, no
Hugging Face pinning, no new dependency. The HF-pinned `fetch` (revision + sha256) and any
manifest-integrated / signed `contamination` predicate are deliberate later decisions, the same
"unsigned-read-only first" posture the `diff` fold-in took. The `modified` category and corpus-internal
dedup are out of scope here (dedup is its own roadmapped command).
