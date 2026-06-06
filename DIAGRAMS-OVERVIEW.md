# Attestrum — all 47 diagrams in one map

A single mega-flowchart showing every Mermaid diagram in the Attestrum project as a node, with cross-area links showing how they reference each other. Useful for orientation, for spotting "where does this fit?" gaps, and for figuring out the order to read diagrams when you're new.

This file is **meta** (it describes diagrams, not code), so it lives at the repo root rather than under `docs/diagrams/` — that keeps the diagram-linter from trying to enforce frontmatter / source-of-truth / forward-reference rules on it. Because it is not linter-checked, **it is maintained by hand**: when you add a diagram under `docs/diagrams/`, add a node here too (and bump the count in the title).

---

## Legend

**Arrows:**

| Arrow | Meaning |
|---|---|
| `A --> B` (solid) | Data or control flows from A into B. The output of A is the input of B. |
| `A -.-> B` (dotted) | A *references* a contract that B defines. B is the spec; A is a consumer. |
| `A ==> B` (thick) | A is a higher-level overview that B decomposes into (zoom-in relationship). |

**Node colors** (track each diagram's `source_of_truth` + PROTECTED status):

| Color | Meaning |
|---|---|
| Orange border, peach fill | **PROTECTED** (CLAUDE.md §4). Changing it invalidates every prior corpus; requires the `Protected-system-change:` commit footer. |
| Green border, mint fill | `source_of_truth: code`. Live, shipped contract. Code is authoritative; the diagram is a derived view. |
| Amber border, cream fill | `source_of_truth: diagram`. Spec the code must implement. Flips to `code` when the matching implementation lands. |
| Blue border, ice fill | `source_of_truth: spec`. An external spec (RFC 6962, in-toto v1, Sigstore Bundle v0.3) is authoritative; drift means our implementation is wrong, not the diagram. |

**Subgraphs** group diagrams by area: `overview`, `sprint-1`…`sprint-6`, `attestations`, `binding`, `index`, `lookback`, `website-fuzzy`.

---

## The map

```mermaid
flowchart LR
  %% =====================================================================
  %% OVERVIEW — top-level orientation diagrams (14)
  %% =====================================================================
  subgraph OV [overview]
    direction TB
    SYSTEM["system<br/>(flowchart)"]
    CRATE_DEPS["crate-deps<br/>(flowchart)"]
    BUILD_HAPPY["build-happy-path<br/>(flowchart)"]
    OV_PROVE["prove-pipeline<br/>(flowchart)"]
    SIGNAL_DEC["signal-decision<br/>(stateDiagram-v2)"]
    SIGSTORE_SV["sigstore-sign-verify<br/>(sequenceDiagram)"]
    TAKEDOWN["takedown-witness<br/>(flowchart)"]
    HUB["hub-publish<br/>(sequenceDiagram)"]
    OV_FP["fingerprint-pipeline<br/>(flowchart)"]
    CAS_LAYOUT["cas-layout<br/>(flowchart) PROTECTED"]
    BSP_CI["build-sign-publish-ci<br/>(flowchart)"]
    CROISSANT["croissant-document-shape<br/>(flowchart)"]
    CYCLONEDX["cyclonedx-document-shape<br/>(flowchart)"]
    STATIC_PUB["static-publish<br/>(flowchart)"]
  end

  %% =====================================================================
  %% SPRINT 1 — scaffolding + signal parsers (7)
  %% =====================================================================
  subgraph S1 [sprint-1]
    direction TB
    WORKSPACE["workspace-layout<br/>(flowchart)"]
    CORE_TYPES["attestrum-core-types<br/>(classDiagram)"]
    SP_PIPE["signal-parser-pipeline<br/>(flowchart)"]
    LINTER["ci-diagram-linter<br/>(sequenceDiagram)"]
    ROBOTS["robots-txt-state<br/>(stateDiagram-v2)"]
    AITXT["ai-txt-rules<br/>(flowchart)"]
    TDMREP["tdmrep-resolution<br/>(sequenceDiagram)"]
  end

  %% =====================================================================
  %% SPRINT 2 — streaming hash + CAS + Merkle (3)
  %% =====================================================================
  subgraph S2 [sprint-2]
    direction TB
    HASH["hash-stream<br/>(sequenceDiagram)"]
    CAS_WRITE["cas-write-atomicity<br/>(sequenceDiagram) PROTECTED"]
    MERKLE["merkle-construction<br/>(flowchart) PROTECTED"]
  end

  %% =====================================================================
  %% SPRINT 3 — manifest + pipeline + CLI (6)
  %% =====================================================================
  subgraph S3 [sprint-3]
    direction TB
    MANIFEST["manifest-schema<br/>(erDiagram) PROTECTED"]
    CAS_WRITE_PATH["cas-write-path<br/>(sequenceDiagram)"]
    RAYON["rayon-pipeline<br/>(flowchart)"]
    BUILD_CLI["attestrum-build-cli<br/>(sequenceDiagram)"]
    INSPECT["attestrum-inspect-lifecycle<br/>(stateDiagram-v2)"]
    SHARDING["sharding<br/>(flowchart)"]
  end

  %% =====================================================================
  %% SPRINT 4 — predicate types + sign/verify (3)
  %% =====================================================================
  subgraph S4 [sprint-4]
    direction TB
    PRED3["predicate-three-types<br/>(classDiagram) PROTECTED"]
    SIGN_FLOW["sign-flow<br/>(sequenceDiagram)"]
    VERIFY_FLOW["verify-flow<br/>(sequenceDiagram)"]
  end

  %% =====================================================================
  %% SPRINT 5 — fingerprint + prove + wasm (4)
  %% =====================================================================
  subgraph S5 [sprint-5]
    direction TB
    S5_FP["fingerprint-pipeline<br/>(flowchart) PROTECTED"]
    S5_PROVE["prove-pipeline<br/>(flowchart)"]
    PROVE_SIGN_CI["prove-sign-ci-interop<br/>(sequenceDiagram)"]
    WASM_FP["wasm-fingerprint-kernel<br/>(flowchart) PROTECTED"]
  end

  %% =====================================================================
  %% SPRINT 6 — visitor verification (1)
  %% =====================================================================
  subgraph S6 [sprint-6]
    direction TB
    VERIFY_PAGE["verify-page<br/>(sequenceDiagram)"]
  end

  %% =====================================================================
  %% ATTESTATIONS — predicate URIs (1)
  %% =====================================================================
  subgraph ATT [attestations]
    PREDICATES["predicate-relationships<br/>(flowchart)"]
  end

  %% =====================================================================
  %% BINDING — corpus-to-model binding (1)
  %% =====================================================================
  subgraph BND [binding]
    BINDING["model-binding-and-chain-walk<br/>(flowchart)"]
  end

  %% =====================================================================
  %% INDEX — fuzzy-lookup sidecars (2)
  %% =====================================================================
  subgraph IDX [index]
    direction TB
    IDX_BQ["build-and-query<br/>(flowchart)"]
    IDX_FMT["sidecar-format<br/>(erDiagram)"]
  end

  %% =====================================================================
  %% LOOKBACK — public fuzzy-search demo (4)
  %% =====================================================================
  subgraph LB [lookback]
    direction TB
    LB_ARCH["lookback-architecture<br/>(flowchart)"]
    LB_SEAL_TOPO["seal-topology<br/>(flowchart)"]
    LB_SEAL["wikitext-seal-pipeline<br/>(flowchart)"]
    LB_PUB["wikitext-publish-pipeline<br/>(sequenceDiagram)"]
  end

  %% =====================================================================
  %% WEBSITE-FUZZY — bounded near-match demo corpus (1)
  %% =====================================================================
  subgraph WF [website-fuzzy]
    FUZZY_GEN["bounded-corpus-gen<br/>(flowchart)"]
  end

  %% ============= Sprint 1 internal + Sprint 1 -> Overview ==============
  ROBOTS --> SP_PIPE
  AITXT --> SP_PIPE
  TDMREP --> SP_PIPE
  CORE_TYPES -.-> WORKSPACE
  CORE_TYPES -.-> SP_PIPE
  SP_PIPE --> SIGNAL_DEC
  CORE_TYPES -.-> CRATE_DEPS
  WORKSPACE -.-> CRATE_DEPS
  LINTER -.-> SYSTEM

  %% ============= Sprint 2 internal + -> Overview =======================
  HASH --> CAS_WRITE
  CAS_WRITE -.-> CAS_LAYOUT

  %% ============= Sprint 3 — composition of Sprint 1+2 + new ============
  HASH ==> RAYON
  MERKLE ==> RAYON
  MANIFEST ==> RAYON
  CAS_WRITE -.-> CAS_WRITE_PATH
  HASH --> CAS_WRITE_PATH
  CAS_WRITE_PATH ==> RAYON
  MANIFEST -.-> CAS_LAYOUT
  RAYON --> BUILD_CLI
  MANIFEST --> INSPECT
  RAYON --> SHARDING
  SIGNAL_DEC -.-> RAYON
  RAYON -.-> BUILD_HAPPY
  MANIFEST -.-> BUILD_HAPPY
  MERKLE -.-> BUILD_HAPPY

  %% ============= Attestations + Sprint 4 — predicates, sign, verify ====
  PREDICATES ==> PRED3
  PREDICATES --> OV_PROVE
  SIGSTORE_SV ==> SIGN_FLOW
  SIGSTORE_SV ==> VERIFY_FLOW
  PRED3 --> SIGN_FLOW
  SIGN_FLOW --> VERIFY_FLOW
  RAYON --> SIGN_FLOW
  SIGSTORE_SV -.-> BUILD_HAPPY

  %% ============= Overview prove pipeline + Sprint 5 fingerprint/prove ==
  OV_FP --> OV_PROVE
  MANIFEST -.-> OV_PROVE
  MERKLE -.-> OV_PROVE
  SIGSTORE_SV -.-> OV_PROVE
  OV_FP ==> S5_FP
  OV_PROVE ==> S5_PROVE
  S5_FP --> S5_PROVE
  S5_FP --> WASM_FP
  S5_PROVE --> PROVE_SIGN_CI
  SIGN_FLOW --> PROVE_SIGN_CI

  %% ============= Index — accelerates prove =============================
  IDX_FMT -.-> IDX_BQ
  S5_FP --> IDX_BQ
  IDX_BQ --> S5_PROVE

  %% ============= Binding — corpus-to-model =============================
  PRED3 -.-> BINDING
  SIGN_FLOW --> BINDING
  MANIFEST -.-> BINDING

  %% ============= Emit document shapes + publish targets ================
  MANIFEST -.-> CROISSANT
  MANIFEST -.-> CYCLONEDX
  SIGN_FLOW --> HUB
  SIGN_FLOW --> STATIC_PUB
  CROISSANT --> HUB
  CYCLONEDX --> HUB
  BUILD_CLI -.-> HUB
  BUILD_CLI --> BSP_CI
  SIGN_FLOW --> BSP_CI

  %% ============= Takedown + Sprint 6 verify page =======================
  SIGSTORE_SV --> TAKEDOWN
  MERKLE -.-> TAKEDOWN
  VERIFY_FLOW ==> VERIFY_PAGE
  SIGSTORE_SV -.-> VERIFY_PAGE

  %% ============= Lookback — the public demo (seal → sign → publish) ====
  LB_ARCH ==> LB_SEAL_TOPO
  LB_ARCH ==> LB_SEAL
  LB_ARCH ==> LB_PUB
  RAYON -.-> LB_SEAL
  LB_SEAL --> LB_PUB
  SIGN_FLOW --> LB_PUB
  HUB -.-> LB_PUB
  S5_PROVE --> LB_ARCH
  PROVE_SIGN_CI -.-> LB_ARCH

  %% ============= Website-fuzzy — the in-browser near-match =============
  S5_FP --> FUZZY_GEN
  WASM_FP -.-> FUZZY_GEN
  FUZZY_GEN --> LB_ARCH
  WASM_FP --> LB_ARCH

  %% ============= System overview decomposes into the rest ==============
  SYSTEM ==> CRATE_DEPS
  SYSTEM ==> BUILD_HAPPY
  SYSTEM ==> OV_PROVE

  %% ============= Styling ===============================================
  classDef protected fill:#fff4e8,stroke:#c0633a,stroke-width:3px,color:#6a1a00
  classDef code fill:#e8f5ea,stroke:#2d7d3e,color:#0a3a14
  classDef draft fill:#fff9e8,stroke:#b07d00,color:#4a3300
  classDef spec fill:#e8f1fb,stroke:#2d5d9e,color:#0a223a

  class CAS_LAYOUT,CAS_WRITE,MERKLE,MANIFEST,PRED3,S5_FP,WASM_FP protected
  class SIGSTORE_SV spec
  class SYSTEM,BUILD_HAPPY,OV_PROVE,OV_FP,TAKEDOWN,WORKSPACE,SP_PIPE,LB_ARCH,LB_SEAL_TOPO draft
  class CRATE_DEPS,SIGNAL_DEC,HUB,BSP_CI,CROISSANT,CYCLONEDX,STATIC_PUB,CORE_TYPES,LINTER,ROBOTS,AITXT,TDMREP,HASH,CAS_WRITE_PATH,RAYON,BUILD_CLI,INSPECT,SHARDING,SIGN_FLOW,VERIFY_FLOW,S5_PROVE,PROVE_SIGN_CI,VERIFY_PAGE,PREDICATES,BINDING,IDX_BQ,IDX_FMT,LB_PUB,LB_SEAL,FUZZY_GEN code
```

---

## Notes — what's going on in each cluster

### overview/ — orientation layer (14 diagrams)

The mental model for the project. The originals: **`system`** (the 30-second elevator pitch, points outward to everything), **`crate-deps`** (the workspace dep graph, derived from the `Cargo.toml`s), **`build-happy-path`** + **`prove-pipeline`** (the two HEADLINE flows — `build` seals a corpus, `prove` answers "is doc X in corpus Y?"; **`prove-pipeline` is THE wedge** auditors care about), **`signal-decision`** (collapses per-parser verdicts into `Included | Excluded | NeedsReview`), **`sigstore-sign-verify`** (the Sigstore Bundle v0.3 + in-toto v1 wire format — `source_of_truth: spec`, the blue node — verifiable by any cosign v3+ with no Attestrum install), **`takedown-witness`** + **`hub-publish`** (witness ledger + HF Hub publish), **`fingerprint-pipeline`** (the planning-era overview; its shipped form is `sprint-5/fingerprint-pipeline`), and **`cas-layout`** (PROTECTED `.attestrum/cas/` directory layout).

Added since the originals: **`build-sign-publish-ci`** (the build→sign→publish CI dry-run), **`croissant-document-shape`** + **`cyclonedx-document-shape`** (the Croissant 1.0 + CycloneDX 1.6 ML-BOM documents `attestrum-emit` emits from the manifest), and **`static-publish`** (the `--target static` self-hosting publish path alongside `hub-publish`).

### sprint-1/ — scaffolding + top-3 signal parsers (7 diagrams)

`workspace-layout` and `attestrum-core-types` are the substrate every later crate consumes. The three parser state machines (`robots-txt-state`, `ai-txt-rules`, `tdmrep-resolution`) feed the cross-parser `signal-parser-pipeline`, which aggregates into the overview's `signal-decision`. `ci-diagram-linter` is the meta diagram for the custom Rust linter at `tools/diagram-linter/` that enforces the diagrams-first rule — hence `LINTER -.-> SYSTEM` (it governs every diagram). NB: the signal parsers are implemented + tested but **not yet wired into the `build` pipeline** (the pipeline records caller-supplied signals).

### sprint-2/ — hash + atomic CAS + Merkle (3 diagrams)

Tiny but consequential — the **determinism foundation**. `hash-stream` is the BLAKE3+SHA-256 streaming hasher (tee, no buffering). `cas-write-atomicity` is the PROTECTED single-put atomicity contract (tmp + rename + fsync). `merkle-construction` is the PROTECTED RFC 6962 binary Merkle over BLAKE3. Byte-identical output across the 4-target CI matrix (linux-x86_64-glibc, linux-aarch64-glibc, macos-aarch64-darwin, linux-x86_64-musl) depends on every byte these three commit to.

### sprint-3/ — manifest + pipeline + CLI (6 diagrams)

The **composition layer**. `manifest-schema` is the PROTECTED 18-column Parquet schema. `cas-write-path` is how the pipeline calls `CasStore::put` from N parallel Rayon workers. `rayon-pipeline` is the three-stage fold-reduce build that wires signals + hash/CAS/Merkle + manifest into a single deterministic build. `attestrum-build-cli` is the user entry; `attestrum-inspect-lifecycle` is the read-only reader CLI; `sharding` wraps the pipeline with `attestrum plan` + `attestrum merge`.

### sprint-4/ — predicate types + sign/verify (3 diagrams)

`predicate-three-types` is the PROTECTED classDiagram of the three in-toto predicate types (training-corpus / inclusion-proof / non-inclusion-proof at `v0.3`; §4 — a schema change requires a version bump + migration). `sign-flow` is the DSSE-wrapped Sigstore Bundle v0.3 emission half; `verify-flow` is the verify half with TrustRoot cache. Together they are the shipped implementation of the overview's `sigstore-sign-verify` spec.

### sprint-5/ — fingerprint + prove + wasm (4 diagrams)

The shipped fingerprinting + proving layer. `fingerprint-pipeline` is the PROTECTED text/image/ISCC pipeline (NFC + MinHash/SimHash + ISCC composition — its text-normalization is §4-locked). `prove-pipeline` is exact + fuzzy match + non-inclusion + alternate manifest sources. `prove-sign-ci-interop` mints the signed inclusion-proof showcase and proves third-party cosign verifies it. `wasm-fingerprint-kernel` is the PROTECTED text-MinHash kernel compiled to `wasm32` (byte-identity-gated in CI) for the attestrum.com near-match demo.

### sprint-6/ — visitor verification (1 diagram)

`verify-page` is the `verify.html` visitor-verification handoff — the static page that hands a visitor a ready-to-paste stock-`cosign` command, the implementation surface of `verify-flow` for non-engineers.

### attestations/ — predicate URIs (1 diagram)

`predicate-relationships` shows how the three Attestrum predicate URIs reference each other (the overview that `sprint-4/predicate-three-types` decomposes into). Feeds both `sigstore-sign-verify` (the in-toto payload) and `prove-pipeline` (proofs USE these predicate types).

### binding/ — corpus-to-model binding (1 diagram)

`model-binding-and-chain-walk` is the `model-binding/v0.1` in-toto Statement that binds a sealed corpus to a model (`attestation_digest_of_bundle`, `bind`, and the signed chain walk) — built on `sign-flow` and the predicate types.

### index/ — fuzzy-lookup sidecars (2 diagrams)

`sidecar-format` is the v1 on-disk `.idx` layout (minhash / perceptual / iscc sub-indexes; raw little-endian, BLAKE3-sealed, NOT a §4 change — a derived artifact). `build-and-query` is the standalone `attestrum index build` plus the `attestrum prove` fuzzy fast-path with exhaustive fallback. The index accelerates `prove` without touching the signed-proof bytes.

### lookback/ — the public fuzzy-search demo (4 diagrams)

`lookback-architecture` is the end-to-end demo overview; `seal-topology` is the seal-local / sign-in-cloud / publish-to-HF topology; `wikitext-seal-pipeline` is the WikiText-103 seal generator; `wikitext-publish-pipeline` is the gated build→sign→publish. This is Phase A made real (the published `Attestrum/wikitext-103-sealed` corpus the live attestrum.com demo checks against). `lookback-architecture` + `seal-topology` are still `source_of_truth: diagram` (the demo's evolving contract); the two pipeline diagrams are `code`.

### website-fuzzy/ — bounded near-match demo corpus (1 diagram)

`bounded-corpus-gen` is the tracked generator (`tools/fuzzy-web-gen`) that turns the showcase passages into `fuzzy-index.json` using the same kernel the wasm compiles from. With `sprint-5/wasm-fingerprint-kernel`, it powers the LIVE in-browser paste-your-own near-match on attestrum.com (both feed `lookback-architecture`).

---

## How to read this map for different purposes

**"I'm new — where do I start?"**
1. `overview/system` (the 30-second view)
2. `overview/crate-deps` (which crate does what)
3. `overview/build-happy-path` (the canonical successful flow)
4. `sprint-1/workspace-layout` + `sprint-1/attestrum-core-types` (the substrate)
5. Then follow the arrows from `build-happy-path` to learn how a build actually works, then `overview/prove-pipeline` → `sprint-5/prove-pipeline` for the wedge.

**"I'm about to touch a PROTECTED zone"** (the orange nodes)
- Read the PROTECTED diagrams FIRST: `cas-layout`, `cas-write-atomicity`, `merkle-construction`, `manifest-schema`, `predicate-three-types`, `sprint-5/fingerprint-pipeline`, `wasm-fingerprint-kernel`.
- Each documents a corpus-incompatible contract; any change requires the `Protected-system-change:` commit footer per CLAUDE.md §4.
- Pay attention to the dotted arrows INTO these diagrams — those are the consumers you'd break.

**"I'm writing a new diagram"**
- Add the file under `docs/diagrams/<area>/<topic>.md` with the 5-field frontmatter (`title`, `models`, `source_of_truth`, `last_verified`, `diagram_type`).
- Then add a node here (this hand-maintained map) and bump the count in the title — this file is NOT linter-checked, so nothing else will catch the omission.

**"I'm planning new work"**
- The amber (`draft`) nodes are diagrams whose implementation hasn't fully landed or that remain the contract the code tracks: `system`, `build-happy-path`, `prove-pipeline` (overview), `fingerprint-pipeline` (overview), `takedown-witness`, `workspace-layout`, `signal-parser-pipeline`, `lookback-architecture`, `seal-topology`. Most Sprint 4–5 work has flipped to green (`code`); `takedown-witness` is the main Sprint 6 flow still ahead.

**"I'm debugging a determinism failure"**
- Walk the build arrows backwards from the output (`manifest-schema` + `merkle-construction`) toward the input (`hash-stream` + the signal parsers). Every diagram on that path is determinism-critical — check each one's `source_of_truth: code` claim is still accurate.

---

## Cross-area links called out (the most important inter-subgraph arrows)

- `sprint-1::SP_PIPE → overview::SIGNAL_DEC` — the parsers feed the aggregator.
- `sprint-2::CAS_WRITE -.-> overview::CAS_LAYOUT` — single-put atomicity realizes the documented directory layout.
- `sprint-2::HASH ==> sprint-3::RAYON`, `MERKLE ==> RAYON`, `sprint-3::MANIFEST ==> RAYON` — hash + Merkle + manifest are the per-worker / seal steps of the build pipeline.
- `sprint-3::RAYON -.-> overview::BUILD_HAPPY` — the rayon-pipeline IS what `build-happy-path` describes at a higher level.
- `overview::SIGSTORE_SV ==> sprint-4::SIGN_FLOW / VERIFY_FLOW` — the spec decomposes into the shipped sign + verify halves.
- `attestations::PREDICATES ==> sprint-4::PRED3` — the predicate-URI overview decomposes into the PROTECTED three-type classDiagram.
- `overview::OV_FP ==> sprint-5::S5_FP ==> sprint-5::WASM_FP` — the fingerprint pipeline's planning view → shipped impl → wasm-compiled kernel.
- `sprint-5::S5_FP → index::IDX_BQ → sprint-5::S5_PROVE` — fingerprints build the sidecar index that accelerates prove.
- `sprint-5::S5_FP / WASM_FP → website-fuzzy::FUZZY_GEN → lookback::LB_ARCH` — the same kernel generates the bounded corpus and runs in-browser for the live near-match demo.
- `sprint-4::SIGN_FLOW → lookback::LB_PUB` — the gated build→sign→publish that minted the public WikiText corpus.

---

## File map

| File | Path | Purpose |
|---|---|---|
| This file | `DIAGRAMS-OVERVIEW.md` | Repo-root meta-map of all diagrams (hand-maintained) |
| Source diagrams | `docs/diagrams/{overview,sprint-1..6,attestations,binding,index,lookback,website-fuzzy}/*.md` | The individual diagram files (linter-enforced) |
| PNG mirror | `diagrams-png/` | Gitignored, regenerated by `bash tools/render-diagrams.sh` |
