---
title: "attestrum-index build + query flows (standalone index build; prove fast-path with exhaustive fallback)"
models: "crates/attestrum-index/src/build.rs, crates/attestrum-index/src/query.rs, crates/attestrum-prove/src/lib.rs"
source_of_truth: code
last_verified: ccf4310 2026-06-12
diagram_type: flowchart
---

# Index build + query flows

Source of truth: `code` — the build/query modules + the prove dispatch fast-path have landed;
this diagram is the derived view, re-verify when they change. The index is **discovery-grade
acceleration**: it only
changes *which candidates get scored*. The exact recheck (`minhash_jaccard_ppm` ≥ 850000,
`min(phash, blockhash) ≤ 6`, `iscc_composite_distance ≤ 4`) and the signed-proof tail
(`crates/attestrum-prove/src/lib.rs` `prove()` 436-490) are the **existing, unchanged** code, so
the emitted proof is byte-identical whether or not the index was used.

## Build (`attestrum index build` → `build_all`)

Standalone subcommand, NOT wired into `attestrum build` (the pipeline does not depend on
`attestrum-fingerprint`; wiring it in would force a `pipeline → fingerprint` edge for a
discovery-grade artifact). One pass over the manifest, one fingerprint per leaf, routed to each
applicable sub-index.

```mermaid
flowchart TD
  Start["attestrum index build<br/>--manifest --cas-root -e"] --> RM["attestrum_manifest::read_manifest"]
  RM --> Root["compute BINDING_ROOT<br/>MerkleTree over document_id, row order"]
  Root --> Loop{"for each manifest leaf"}
  Loop -->|"read bytes"| CAS["CasStore::open(document_id)"]
  CAS --> FP["fingerprint_leaf → full bundle"]
  FP --> Route{"route by available signatures"}
  Route -->|"text minhash"| MH["minhash 128xu64 → band 32x4 → BTreeMap"]
  Route -->|"image phash+blockhash"| PC["perceptual → 7-band pigeonhole → BTreeMap"]
  Route -->|"has iscc composite"| IS["iscc → 5-band pigeonhole → BTreeMap"]
  MH --> Loop
  PC --> Loop
  IS --> Loop
  Loop -->|"done"| Write["atomic write 3 sidecars<br/>stage to tmp, fsync, rename, fsync parent"]
  Write --> Report["BuildReport (per-kind leaf + bucket counts)"]
```

## Query (inside prove `dispatch_minhash` / `dispatch_perceptual` / `dispatch_iscc`)

```mermaid
flowchart TD
  Q["dispatch_* (query signature in hand)"] --> Loc{"locate sidecar<br/>(no_index? index_path? cas_root/index/kind/v1.idx)"}
  Loc -->|"none / --no-index"| Exh["exhaustive scan<br/>(existing loop, recall oracle)"]
  Loc -->|"found"| Load["FuzzyIndex::load"]
  Load -->|"load error"| Exh
  Load -->|"ok"| Bind{"BINDING_ROOT == MerkleTree(entries).root()?"}
  Bind -->|"stale / mismatch"| Exh
  Bind -->|"ok"| Band["band the query signature<br/>gather candidate rows from matching buckets"]
  Band --> Recheck["for each candidate: read persisted signature<br/>run UNCHANGED exact recheck + threshold + best"]
  Recheck --> Tail["emit via UNCHANGED prove() tail<br/>(MatchEvidence, confidence, signed proof)"]
  Exh --> Tail
```

**Test obligations** (CLAUDE.md §7/§9):

- `indexed_equals_exhaustive` — for every query the exhaustive path matches, the indexed path
  returns the identical `(leaf_index, jaccard/distance)`. The load-bearing **recall** proof.
- `recall_at_threshold` — a query whose best match is exactly at the locked threshold
  (Jaccard 0.85 / Hamming = k) is still found via the index.
- `stale_binding_falls_back` — corrupt `BINDING_ROOT` → prove silently uses the exhaustive
  path and still emits the correct proof.
- `no_index_flag_forces_exhaustive` — `--no-index` produces a byte-identical proof to the
  indexed run.
- Determinism: `build_all` twice from a fixed manifest → byte-identical sidecars.
