---
title: "attestrum diff — read-only corpus-version delta over two sealed manifests (merge-join on document_id)"
models: "crates/attestrum-diff/src/lib.rs, crates/attestrum-cli/src/commands/diff.rs (planned)"
source_of_truth: diagram
last_verified: e71552c 2026-06-12
diagram_type: flowchart
---

# attestrum diff — corpus-version delta pipeline

Source of truth: `diagram` — this is the contract the code must implement; it
flips to `source_of_truth: code` once `crates/attestrum-diff/` and
`commands/diff.rs` land (§2 drift rule, same commit).

`attestrum diff <manifestA> <manifestB>` answers *"what changed between two
already-sealed corpus states?"* — the read-only, **unsigned** sibling of
`attestrum inspect` (which reasons about one corpus). It reports added /
removed / unchanged documents, multiset (occurrence) shifts, and per-source /
per-modality composition shift, in a byte-reproducible report. Both endpoints
are already cryptographically sealed, so the report names both Merkle roots and
the delta between them.

Nothing here touches a §4 protected system: no Merkle change, no predicate URI,
no manifest schema change, no CAS/ledger layout. It is the reversible *leaf*
(unsigned report), deliberately not the frozen *trunk* (a signed corpus-delta
predicate — deferred behind the high-stakes protocol).

## Identity model, stated plainly (the report declares it)

`document_id` **is** the BLAKE3 content hash, and manifests are written in
canonical `(document_id, occurrence_index)` sort order (`sort_entries`, enforced
across the seal path). So the diff is a **merge-join of two sorted streams**:

- **unchanged** — `document_id` present in both
- **removed** — present in A only
- **added** — present in B only
- **multiset shift** — same `document_id`, different occurrence count (a doc that
  appeared 3× now appears 1×), read from `occurrence_index` run-length

There is **no "modified" category**: in a content-addressed manifest a changed
document is simply a removed old hash plus an added new hash, with nothing
linking them (no caller-stable id exists in the schema). The report states this
mode explicitly — the honesty pattern, never a fuzzy-matched "modified" verdict.

```mermaid
flowchart TD
    A([manifestA.parquet<br/>sealed, sorted by document_id]):::input
    B([manifestB.parquet<br/>sealed, sorted by document_id]):::input

    %% --- validation, mirrors inspect ---
    A --> V{path is_file?}
    B --> V
    V -- no --> E2[[exit 2 — arg error]]:::err
    V -- yes --> M[read_manifest_metadata each<br/>schema_version + writer_profile + Merkle root]:::reused
    M --> SV{schema_version<br/>match & supported?}
    SV -- no --> E8[[exit 8 — schema mismatch]]:::err
    SV -- yes --> OPEN

    %% --- streaming merge-join core ---
    OPEN[open ManifestBatchReader on each<br/>constant-memory, document_id order]:::reused
    OPEN --> CUR[two-stream lockstep walker<br/>advance reader with smaller document_id]:::new
    CUR -->|read error| E1[[exit 1 — runtime error]]:::err
    CUR --> CLS{compare document_id}
    CLS -- in both --> U[unchanged ++<br/>compare occurrence count → multiset shift]:::new
    CLS -- A only --> R[removed ++<br/>record sorted hex example-id]:::new
    CLS -- B only --> D[added ++<br/>record sorted hex example-id]:::new

    %% --- accumulate during the same walk ---
    U --> H[per-version summaries +<br/>composition histograms<br/>modality · source_type ·<br/>source_dataset_id · license · language]:::new
    R --> H
    D --> H

    H --> REP[build DiffReport<br/>identity-mode + deferred-features notes]:::new
    REP --> J[deterministic_json — sorted keys,<br/>sorted hex example-id lists]:::reused
    REP --> MD[human summary text]:::new
    J --> OUT([report.json → --out]):::output
    MD --> STD([summary → stdout, like inspect]):::output

    %% --- declared & deferred (not computed) ---
    subgraph deferred [Declared-and-deferred in the report]
        direction LR
        DF1[no modified category<br/>needs caller-stable id<br/>= §4 schema change]:::defer
        DF2[no near-dup-rate delta<br/>fingerprints not persisted<br/>= memo F2]:::defer
        DF3[no signed corpus-delta predicate<br/>frozen trunk = high-stakes protocol]:::defer
    end
    REP -.declares.-> deferred

    classDef input  fill:#0d3b66,stroke:#4a90d9,color:#fff
    classDef reused fill:#6a1b1b,stroke:#e05a5a,color:#fff
    classDef new    fill:#13344a,stroke:#3aa0d9,color:#fff
    classDef output fill:#1f6f3f,stroke:#3ec072,color:#fff
    classDef err    fill:#3a2a00,stroke:#e0a52e,color:#fff
    classDef defer  fill:#2a2a2a,stroke:#888,color:#ccc
```

**Legend:** 🟦 dark-blue `input` = the two sealed manifests · 🟥 red `reused` =
existing protected/landed code consumed read-only (`read_manifest_metadata`,
`ManifestBatchReader`, `deterministic_json`) · light-blue `new` = the additive
`attestrum-diff` crate + `commands/diff.rs` · 🟩 green `output` = the two report
surfaces · amber = exit paths · grey = declared-and-deferred (stated in the
report, not computed in v1).

## Public API (`crates/attestrum-diff`)

The crate's surface, as the flowchart's nodes map to it:

- `compare` — the streaming merge-join entry point; consumes two canonically
  sorted `ManifestEntry` streams and returns a `DiffReport`.
- `DiffReport` — the whole report: `REPORT_VERSION` tag, `IDENTITY_MODE` string,
  the `DEFERRED` declared-and-deferred list, optional verbatim timestamp, the two
  `VersionSummary` sides, and the `Delta`.
- `VersionSummary` — per-version Merkle root + counts (documents, distinct,
  exact-duplicate, bytes) + composition histograms.
- `Delta` — added / removed / unchanged counts, example-id lists (capped at
  `MAX_EXAMPLES`), `MultisetShift` records (occurrence-count changes), and the
  per-dimension `ShareShift` composition shift.
- `MultisetShift`, `ShareShift` — the two delta-detail records.
- `render_json` — canonical deterministic JSON (via the shared
  `deterministic_json`); `render_summary` — the human stdout summary.

## Reuse boundary

- **Consumed read-only:** `attestrum_manifest::ManifestBatchReader` /
  `read_manifest_metadata` / `manifest_row_count` (landed in `39fa850`), and
  `attestrum_attest::deterministic_json` (sorts keys recursively — the one shared
  determinism primitive; `attestrum-decontaminate` uses it too).
- **Not reused:** `commands/merge.rs::ShardCursor` is private and k-way (merge
  *combines* rows; diff *classifies* them, 2-way). `attestrum-diff` carries its
  own small 2-stream walker for v1. **Follow-up flag:** unify `ShardCursor` and
  the diff walker into one shared sorted-cursor primitive in `attestrum-manifest`
  once both have landed — a small coordinated refactor, out of scope here.

## Exit codes (mirror inspect)

`0` success · `1` runtime/I-O error · `2` argument error (path missing / not a
file) · `8` schema-version mismatch.

## Determinism contract

Same two manifests → byte-identical `report.json` on any machine. No wall-clock
(timestamp only via an explicit `--timestamp`, embedded verbatim); `BTreeMap`-
keyed accumulation; every example-id list sorted (lowercase hex) before emit;
the merge-join walks in `document_id` order so the output never depends on thread
scheduling. CI gate: a committed golden plus a double-run byte-compare, modelled
on the `croissant` / `api-surface` golden tests.
