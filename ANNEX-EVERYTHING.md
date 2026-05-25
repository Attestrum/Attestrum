# Attestrum — the everything map

Every workflow in Attestrum, fused into one canvas. Operator types `attestrum build` (top-left) → publishes to Hugging Face → a third party with no Attestrum install opens `verify.html` and gets a green check (right side). Side branches cover `prove`, `takedown`, the 18-column manifest, the 4-target CI determinism matrix, and the 14-crate workspace.

**Scale**: ~140 nodes, ~180 edges, 18 phase-subgraphs. Renders to a tall PNG (~5–10 MB at scale=3). Will not fit on one screen — scroll, zoom, or open the rendered PNG in Preview at fit-window then click around.

**This is a meta-document**: it lives at the repo root, not under `docs/diagrams/`, so the linter's frontmatter / source-of-truth / forward-reference rules don't apply. The individual source diagrams under `docs/diagrams/` remain the authoritative per-area contracts; this file is the orientation poster.

---

## Legend

| Visual | Meaning |
|---|---|
| Orange-bordered node, peach fill | PROTECTED per CLAUDE.md §4. Changing the contract invalidates every prior corpus. |
| Blue-bordered node, pale-blue fill | An output artifact that LEAVES the build (a file on disk or a network payload). |
| Pink-bordered node, pink fill | A CLI command (where the operator or auditor types something). |
| Plain box | An internal step or value. |
| Solid arrow `→` | Data or control flows from one step to the next. |
| Decision diamond `{...?}` | A branch in the flow. |
| Numbered `[N]` callouts | Anchored to the narrative section below. Read the diagram + the corresponding numbered note together. |

---

## The map

```mermaid
flowchart TB
  %% ===================================================================
  %% [1] PHASE 1 — operator input
  %% ===================================================================
  subgraph P1 ["[1] Phase 1 — operator input"]
    direction LR
    OP_CMD["attestrum build<br/>--corpus corpus.toml<br/>--workspace path<br/>--source-date-epoch N"]
    CORPUS_TOML["corpus.toml<br/>BUILD-PLAN §8.3"]
    CE_LIST["Vec CorpusEntry<br/>source_uri + ContentSource + caller metadata"]
    OP_CMD --> CORPUS_TOML --> CE_LIST
  end

  %% ===================================================================
  %% [2] PHASE 2 — signal sources fetched per origin
  %% ===================================================================
  subgraph P2 ["[2] Phase 2 — signal sources fetched per origin"]
    direction LR
    SIG_ROBOTS["robots.txt<br/>origin/robots.txt"]
    SIG_AITXT["ai.txt<br/>origin/.well-known/ai.txt"]
    SIG_TDMR["TDMRep well-known JSON<br/>origin/.well-known/tdmrep.json"]
    SIG_TDMR_H["TDMRep HTTP header<br/>X-TDMRep-Reservation"]
    SIG_TDMR_M["TDMRep meta tag<br/>HTML head"]
    SIG_IPTC["IPTC PLUS DMI<br/>via XMP"]
    SIG_C2PA["C2PA training-mining<br/>assertion in C2PA manifest"]
    SIG_RSL["RSL well-known doc"]
    SIG_LICCIUM["Liccium TDM-AI<br/>ISCC sidecar"]
    SIG_CFCS["Cloudflare AI Crawl Control<br/>response headers"]
  end

  %% ===================================================================
  %% [3] PHASE 3 — per-signal parsers (attestrum-signals)
  %% ===================================================================
  subgraph P3 ["[3] Phase 3 — per-signal parsers via SignalParser trait"]
    direction LR
    PARSE_ROBOTS["RobotsParser RFC 9309<br/>404=Unknown / empty=Unknown / matched group=Allow|Disallow"]
    PARSE_AITXT["AiTxtParser Spawning<br/>per-UA rules"]
    PARSE_TDMR["TdmrepParser 3-layer<br/>JSON → HTTP hdr override → meta override"]
    VERDICT["SignalVerdict<br/>Allowed | Disallowed | Unknown"]
    SIG_ROBOTS --> PARSE_ROBOTS --> VERDICT
    SIG_AITXT --> PARSE_AITXT --> VERDICT
    SIG_TDMR --> PARSE_TDMR
    SIG_TDMR_H --> PARSE_TDMR
    SIG_TDMR_M --> PARSE_TDMR --> VERDICT
  end

  %% ===================================================================
  %% [4] PHASE 4 — signal aggregation → SignalDecision
  %% ===================================================================
  subgraph P4 ["[4] Phase 4 — aggregation → SignalDecision per doc"]
    direction TB
    AGG["For each doc:<br/>collect verdicts + apply ruleset"]
    RSET{"ruleset?<br/>strict / audit-only / permissive"}
    DEC_REJ["StrictReject<br/>any Disallowed under strict"]
    DEC_INC["Included<br/>Allowed OR no preference under permissive"]
    DEC_AUDIT["AuditFlag<br/>Unknown under audit-only — reviewer decides"]
    DEC_FINAL["SignalDecision<br/>included: bool, exclusion_reason: Opt String,<br/>signals: ManifestSignals (12 fields)"]
    VERDICT --> AGG
    CE_LIST --> AGG
    AGG --> RSET
    RSET --> DEC_REJ
    RSET --> DEC_INC
    RSET --> DEC_AUDIT
    DEC_REJ --> DEC_FINAL
    DEC_INC --> DEC_FINAL
    DEC_AUDIT --> DEC_FINAL
  end

  %% ===================================================================
  %% [5] PHASE 5 — Rayon parallel hash + CAS
  %% ===================================================================
  subgraph P5 ["[5] Phase 5 — Rayon par_iter parallel hash + CAS put"]
    direction TB
    PAR["par_iter().enumerate() over Vec CorpusEntry<br/>NO Mutex Vec — per-worker fold"]
    READ["worker: read bytes from ContentSource Path or Bytes"]
    SH_INIT["new blake3::Hasher + sha2::Sha256"]
    SH_LOOP["loop: read 8 KiB → tee into both hashers"]
    SH_OUT["StreamHash<br/>blake3: [u8;32], sha256: [u8;32], size_bytes: u64"]
    CAS_STAT["stat final_path<br/>root/cas/blake3/ab/cd/hex.bin"]
    CAS_PRESENT{"final path<br/>exists?"}
    CAS_FAST["idempotent fast path<br/>return Ok"]
    CAS_TMP["write contents to<br/>root/tmp/.attestrum-tmp.pid-n-nanos"]
    CAS_FSYNC1["fsync data"]
    CAS_RENAME["atomic rename(2) tmp → final"]
    CAS_FSYNC2["fsync parent dir<br/>best-effort"]
    ROW["build ManifestEntry<br/>doc_id=blake3, sha256, size, modality,<br/>signals, included, exclusion_reason,<br/>input_ordinal=enumerate_idx"]
    PUSH["push into per-worker Vec ManifestEntry<br/>from fold — no lock"]
    PAR --> READ --> SH_INIT --> SH_LOOP --> SH_OUT
    SH_OUT --> CAS_STAT --> CAS_PRESENT
    CAS_PRESENT -->|yes| CAS_FAST
    CAS_PRESENT -->|no| CAS_TMP --> CAS_FSYNC1 --> CAS_RENAME --> CAS_FSYNC2
    CAS_FSYNC2 --> ROW
    CAS_FAST --> ROW
    SH_OUT --> ROW
    ROW --> PUSH
    DEC_FINAL --> ROW
    CE_LIST --> PAR
  end

  %% ===================================================================
  %% [6] PHASE 6 — epilogue join + sort + bind
  %% ===================================================================
  subgraph P6 ["[6] Phase 6 — single-thread epilogue join + bind"]
    direction TB
    REDUCE["reduce: Vec::append per-worker Vecs<br/>O(N) memcpy once"]
    SORT_IN["sort_by_key(input_ordinal)<br/>restore canonical input order"]
    AOI["assign_occurrence_indices<br/>walk in input-order, per-digest rank 0,1,2,..."]
    SORT_CANON["sort_entries<br/>(document_id ASC, occurrence_index ASC)<br/>canonical on-disk order"]
    REDUCE --> SORT_IN --> AOI --> SORT_CANON
    PUSH --> REDUCE
  end

  %% ===================================================================
  %% [7] PHASE 7 — Parquet manifest write (PROTECTED)
  %% ===================================================================
  subgraph P7 ["[7] Phase 7 — PROTECTED Parquet manifest write"]
    direction TB
    SCHEMA["18-col Arrow SchemaRef<br/>document_id+sha256: FixedSizeBinary(32)<br/>size_bytes: UInt64<br/>modality+source_type: Int8<br/>mime_type+source_url+...: Utf8 nullable<br/>fetched_at: Int64 nullable<br/>signals: STRUCT 12 fields<br/>included: Boolean<br/>chunk_refs: List FixedSizeBinary(32) nullable<br/>input_ordinal: UInt64<br/>occurrence_index: UInt32"]
    WRITER_CFG["WriterProperties (PROTECTED)<br/>PARQUET_1_0 / ZSTD level 3 / dict OFF global<br/>stats OFF global / bloom OFF<br/>row_group 1M / data_page 1MB+20K rows<br/>created_by attestrum-manifest/0.1.0<br/>KeyValue as Vec NOT HashMap"]
    KV["KeyValue metadata sorted Vec<br/>attestrum.manifest.schema_version=1<br/>attestrum.writer.profile=parquet-rs-55-zstd3-plain-v1"]
    RBATCH["entries_to_record_batch<br/>FixedSizeBinaryBuilder + StringBuilder + Int8Builder<br/>+ ListBuilder + StructBuilder per column"]
    PWRITE["ArrowWriter::try_new(file, schema, props)<br/>.write(batch).close()"]
    MANIFEST["manifest.parquet<br/>at output_dir/manifest.parquet"]
    SORT_CANON --> RBATCH --> PWRITE
    SCHEMA --> PWRITE
    WRITER_CFG --> PWRITE
    KV --> WRITER_CFG
    PWRITE --> MANIFEST
  end

  %% ===================================================================
  %% [8] PHASE 8 — Merkle commitment (PROTECTED)
  %% ===================================================================
  subgraph P8 ["[8] Phase 8 — PROTECTED RFC 6962 Merkle over BLAKE3"]
    direction TB
    LEAVES["extract sorted document_id column<br/>Vec [u8;32]"]
    M_EMPTY{"n=0?"}
    M_EMPTY_OUT["root = BLAKE3 of empty<br/>af1349b9..."]
    M_LH["for each leaf:<br/>leaf_hash = BLAKE3(0x00 || leaf)"]
    M_LEVEL["current level = Vec leaf_hash"]
    M_PAIRS["pair-and-hash:<br/>node_hash = BLAKE3(0x01 || L || R)"]
    M_ODD{"level odd?"}
    M_CARRY["lone rightmost carried up UNCHANGED<br/>RFC 6962 NOT Bitcoin duplicate"]
    M_NEXT["next level = Vec"]
    M_DONE{"len = 1?"}
    M_ROOT["merkle_root [u8;32]"]
    SORT_CANON --> LEAVES --> M_EMPTY
    M_EMPTY -->|yes| M_EMPTY_OUT
    M_EMPTY -->|no| M_LH --> M_LEVEL --> M_PAIRS --> M_ODD
    M_ODD -->|yes| M_CARRY --> M_NEXT
    M_ODD -->|no| M_NEXT
    M_NEXT --> M_DONE
    M_DONE -->|no| M_PAIRS
    M_DONE -->|yes| M_ROOT
  end

  %% ===================================================================
  %% [9] PHASE 9 — BuildOutput
  %% ===================================================================
  subgraph P9 ["[9] Phase 9 — BuildOutput per build_corpus call"]
    direction LR
    BOUT["BuildOutput<br/>merkle_root, manifest_path,<br/>leaf_count, total_bytes"]
    M_ROOT --> BOUT
    M_EMPTY_OUT --> BOUT
    MANIFEST --> BOUT
  end

  %% ===================================================================
  %% [10] PHASE 10 — in-toto Statement v1
  %% ===================================================================
  subgraph P10 ["[10] Phase 10 — attestrum-attest in-toto Statement v1"]
    direction TB
    SH_MAN["SHA-256 of manifest.parquet"]
    STMT_SUBJ["subject = [{<br/>  name: manifest.parquet,<br/>  digest: {sha256: SH_MAN}<br/>}]"]
    STMT_PRED_T["predicateType =<br/>https://attestrum.com/attestation/training-corpus/v0.1"]
    STMT_PRED["predicate (Attestrum-defined):<br/>merkle_root, document_count,<br/>modality breakdown, license breakdown,<br/>signal coverage, ruleset, source_date_epoch,<br/>builderVersion = attestrum/x.y.z"]
    STMT["in-toto Statement v1 JSON<br/>{_type, subject[], predicateType, predicate}"]
    MANIFEST --> SH_MAN --> STMT_SUBJ --> STMT
    STMT_PRED_T --> STMT
    STMT_PRED --> STMT
    BOUT --> STMT_PRED
  end

  %% ===================================================================
  %% [11] PHASE 11 — DSSE + Sigstore Bundle v0.3
  %% ===================================================================
  subgraph P11 ["[11] Phase 11 — DSSE + Sigstore Bundle v0.3 sign"]
    direction TB
    PAYLOAD["payload = base64(JSON Statement)<br/>payloadType = application/vnd.in-toto+json"]
    OIDC["OIDC id_token from<br/>GitHub / Google / Microsoft"]
    FULCIO["CSR + id_token → Fulcio CA<br/>→ short-lived X.509 cert<br/>+ ephemeral key"]
    DSSE_SIGN["DSSE-sign payload<br/>with ephemeral key"]
    REKOR["submit DSSE envelope +<br/>verificationMaterial → Rekor v2 tile-backed"]
    REKOR_PROOF["signed inclusion proof<br/>+ RFC3161 timestamp"]
    BUNDLE_ASSEMBLE["assemble Bundle v0.3<br/>application/vnd.dev.sigstore.bundle.v0.3+json"]
    BUNDLE["bundle.sigstore.json"]
    STMT --> PAYLOAD --> DSSE_SIGN
    OIDC --> FULCIO --> DSSE_SIGN
    DSSE_SIGN --> REKOR --> REKOR_PROOF --> BUNDLE_ASSEMBLE
    FULCIO --> BUNDLE_ASSEMBLE
    BUNDLE_ASSEMBLE --> BUNDLE
  end

  %% ===================================================================
  %% [12] PHASE 12 — emit sidecars (attestrum-emit)
  %% ===================================================================
  subgraph P12 ["[12] Phase 12 — attestrum-emit sidecar artifacts"]
    direction LR
    EMIT_R["ManifestReader walks<br/>manifest.parquet"]
    EMIT_AGG["aggregations<br/>modality count / size buckets /<br/>domain histogram (publicsuffix) /<br/>license SPDX coverage / signal coverage"]
    EMIT_C["Croissant JSON-LD<br/>croissant.json"]
    EMIT_CARD["dataset card README.md<br/>YAML frontmatter + provenance section"]
    EMIT_V["verify.html<br/>static + embedded WASM cosign-lite"]
    EMIT_53["Article 53 PDF + summary.json<br/>Typst render, pinned font set"]
    EMIT_BOM["CycloneDX 1.7 ML-BOM<br/>attestrum.cdx.json"]
    MANIFEST --> EMIT_R --> EMIT_AGG
    EMIT_AGG --> EMIT_C
    EMIT_AGG --> EMIT_CARD
    EMIT_AGG --> EMIT_V
    EMIT_AGG --> EMIT_53
    EMIT_AGG --> EMIT_BOM
  end

  %% ===================================================================
  %% [13] PHASE 13 — HF Hub publish (attestrum-publish)
  %% ===================================================================
  subgraph P13 ["[13] Phase 13 — attestrum publish to Hugging Face Hub"]
    direction TB
    HF_CMD["attestrum publish --target hf<br/>--dataset org/name<br/>--bundle bundle.sigstore.json"]
    HF_API["POST /api/repos/create<br/>type=dataset name=org/name exist_ok=true"]
    HF_COMMIT["create_commit operations"]
    HF_O1["add README.md"]
    HF_O2["add croissant.json"]
    HF_O3["add attestrum/manifest.parquet"]
    HF_O4["add attestrum/merkle.root"]
    HF_O5["add attestrum/bundle.sigstore.json"]
    HF_O6["add attestrum/verify.html"]
    HF_RES["200 commit OID<br/>+ dataset URL"]
    HF_WH["webhook notification<br/>downstream consumers"]
    HF_CMD --> HF_API --> HF_COMMIT
    HF_COMMIT --> HF_O1
    HF_COMMIT --> HF_O2
    HF_COMMIT --> HF_O3
    HF_COMMIT --> HF_O4
    HF_COMMIT --> HF_O5
    HF_COMMIT --> HF_O6
    EMIT_CARD --> HF_O1
    EMIT_C --> HF_O2
    MANIFEST --> HF_O3
    BOUT --> HF_O4
    BUNDLE --> HF_O5
    EMIT_V --> HF_O6
    HF_COMMIT --> HF_RES --> HF_WH
  end

  %% ===================================================================
  %% [14] PHASE 14 — third-party verify (NO Attestrum install)
  %% ===================================================================
  subgraph P14 ["[14] Phase 14 — third-party verify (cosign only, no Attestrum)"]
    direction TB
    V_OPEN["fresh visitor opens verify.html<br/>in browser"]
    V_FETCH["embedded WASM cosign-lite fetches<br/>bundle + manifest from HF Hub"]
    V_TUF["TrustRoot via TUF refresh"]
    V_CHAIN["verify Fulcio cert chain"]
    V_OIDC["verify OIDC identity claim<br/>--certificate-identity-regexp"]
    V_REKOR["verify Rekor inclusion proof<br/>+ RFC3161 timestamp"]
    V_RES{"all checks OK?"}
    V_GREEN["green check<br/>corpus authentic"]
    V_RED["red X<br/>with diagnostic"]
    HF_WH --> V_OPEN --> V_FETCH --> V_TUF --> V_CHAIN --> V_OIDC --> V_REKOR --> V_RES
    V_RES -->|yes| V_GREEN
    V_RES -->|no| V_RED
  end

  %% ===================================================================
  %% [15] PHASE 15 — attestrum prove (inclusion / non-inclusion)
  %% ===================================================================
  subgraph P15 ["[15] Phase 15 — attestrum prove inclusion or non-inclusion"]
    direction TB
    PR_CMD["attestrum prove DOC --against MANIFEST"]
    PR_INPUT{"input kind?"}
    PR_HEX["BLAKE3 hex → exact match"]
    PR_ISCC["ISCC URI → similarity"]
    PR_PH["perceptual hash → distance"]
    PR_RAW["raw doc → fingerprint via attestrum-fingerprint"]
    PR_FP["FingerprintBundle by modality:<br/>text MinHash+SimHash+ISCC,<br/>image pHash+blockhash+ISCC,<br/>audio chromaprint+ISCC,<br/>video keyframe pHash+ISCC"]
    PR_LOAD["load manifest<br/>local .parquet OR hf://org/name OR registry"]
    PR_Q{"match?"}
    PR_AP["build audit path:<br/>from leaf_index walk sibling chain to root"]
    PR_VER["verify_audit_path:<br/>replay leaf_hash + sibling pairs<br/>+ odd-count carry — accept iff root matches"]
    PR_INC["predicateType = inclusion-proof/v0.1<br/>predicate = {leaf, leaf_index, tree_size, audit_path}"]
    PR_NI["predicateType = non-inclusion-proof/v0.1<br/>predicate = {sorted-neighbor proof,<br/>sibling at insertion point}"]
    PR_SIGN["separate DSSE + Sigstore bundle<br/>proof.sigstore.json"]
    PR_CMD --> PR_INPUT
    PR_INPUT -->|BLAKE3 hex| PR_HEX
    PR_INPUT -->|ISCC URI| PR_ISCC
    PR_INPUT -->|perceptual| PR_PH
    PR_INPUT -->|raw doc| PR_RAW --> PR_FP
    PR_HEX --> PR_LOAD
    PR_ISCC --> PR_LOAD
    PR_PH --> PR_LOAD
    PR_FP --> PR_LOAD
    PR_LOAD --> PR_Q
    PR_Q -->|yes| PR_AP --> PR_VER --> PR_INC --> PR_SIGN
    PR_Q -->|no| PR_NI --> PR_SIGN
    MANIFEST --> PR_LOAD
  end

  %% ===================================================================
  %% [16] PHASE 16 — attestrum takedown
  %% ===================================================================
  subgraph P16 ["[16] Phase 16 — attestrum takedown (rightsholder request)"]
    direction TB
    TD_CMD["attestrum takedown<br/>--doc HASH --reason ... --witness rekor|hub|null"]
    TD_STAND["verify standing<br/>attestrum-ledger"]
    TD_LEAF["append takedown leaf<br/>to local append-only log"]
    TD_WIT{"witness?"}
    TD_REKOR["submit to Rekor v2<br/>predicate takedown/v0.1"]
    TD_HUB["append to HF Hub witness repo<br/>org/dataset-witness/log.jsonl"]
    TD_NULL["local only"]
    TD_NEW["new corpus version v_n+1<br/>prev_root = v_n.merkle_root"]
    TD_CHAIN["cryptographic chain v_n → v_n+1"]
    TD_SIGN["sign new manifest<br/>training-corpus/v0.1"]
    TD_PUB["republish to HF<br/>attestrum publish"]
    TD_CMD --> TD_STAND --> TD_LEAF --> TD_WIT
    TD_WIT -->|rekor| TD_REKOR
    TD_WIT -->|hub| TD_HUB
    TD_WIT -->|null| TD_NULL
    TD_REKOR --> TD_NEW
    TD_HUB --> TD_NEW
    TD_NULL --> TD_NEW
    TD_NEW --> TD_CHAIN --> TD_SIGN --> TD_PUB
    BOUT --> TD_NEW
  end

  %% ===================================================================
  %% [17] PHASE 17 — CI determinism matrix
  %% ===================================================================
  subgraph P17 ["[17] Phase 17 — CI determinism matrix .github/workflows/determinism.yml"]
    direction LR
    CI_T1["linux-x86_64-glibc<br/>ubuntu-24.04"]
    CI_T2["linux-aarch64-glibc<br/>ubuntu-24.04-arm"]
    CI_T3["macos-aarch64-darwin<br/>macos-14"]
    CI_T4["linux-x86_64-musl<br/>alpine:3.20"]
    CI_RUN["each target:<br/>cargo test --workspace<br/>cargo run --example sprint-2-corpus<br/>attestrum build synthetic-1k.toml"]
    CI_ARTS["upload artifacts<br/>merkle-root-LABEL.txt<br/>manifest.parquet"]
    CI_CMP["compare job<br/>cmp -s pairwise across all 4 targets"]
    CI_RES{"all 4 byte-identical?"}
    CI_GREEN["CI green<br/>cross-platform determinism confirmed"]
    CI_FAIL["::error:: divergence<br/>at byte offset N → bisect"]
    CI_T1 --> CI_RUN
    CI_T2 --> CI_RUN
    CI_T3 --> CI_RUN
    CI_T4 --> CI_RUN
    M_ROOT --> CI_RUN
    MANIFEST --> CI_RUN
    CI_RUN --> CI_ARTS --> CI_CMP --> CI_RES
    CI_RES -->|yes| CI_GREEN
    CI_RES -->|no| CI_FAIL
  end

  %% ===================================================================
  %% [18] PHASE 18 — workspace dependency graph
  %% ===================================================================
  subgraph P18 ["[18] Phase 18 — workspace dep graph (14 crates)"]
    direction TB
    C_CORE["attestrum-core<br/>DocumentDigest, Modality, SourceType,<br/>BuildContext, AttestrumError, hex"]
    C_SIG["attestrum-signals<br/>SignalParser, Robots/Aitxt/Tdmrep,<br/>decision aggregator"]
    C_CAS["attestrum-cas PROTECTED<br/>stream_hash, CasStore"]
    C_MERKLE["attestrum-merkle PROTECTED<br/>leaf_hash, node_hash, merkle_root,<br/>audit_path, verify_audit_path"]
    C_MAN["attestrum-manifest PROTECTED<br/>ManifestEntry, sort_entries, assign_*,<br/>write_manifest, read_manifest, io::*"]
    C_PIPE["attestrum-pipeline<br/>build_corpus via Rayon fold+reduce"]
    C_CLI["attestrum-cli<br/>clap: build/inspect/plan/merge/sign/verify/<br/>prove/publish/takedown/emit/fingerprint"]
    C_ATT["attestrum-attest<br/>in-toto Statement + Sigstore Bundle v0.3<br/>+ predicate types"]
    C_EMIT["attestrum-emit<br/>Croissant + Article 53 + CycloneDX<br/>+ dataset card + verify.html"]
    C_PROVE["attestrum-prove<br/>fingerprint match + audit-path<br/>+ inclusion/non-inclusion predicates"]
    C_PUB["attestrum-publish<br/>HuggingFaceTarget + GitHubReleaseTarget"]
    C_LED["attestrum-ledger<br/>tile-based append-only takedown log"]
    C_FP["attestrum-fingerprint<br/>BLAKE3 + ISCC + perceptual + MinHash by modality"]
    C_FPR["attestrum-fingerprint-registry<br/>optional v1+ shared registry"]
    C_SIG --> C_CORE
    C_CAS --> C_CORE
    C_MERKLE --> C_CORE
    C_MAN --> C_CORE
    C_FP --> C_CORE
    C_LED --> C_CORE
    C_PIPE --> C_SIG
    C_PIPE --> C_CAS
    C_PIPE --> C_MAN
    C_PIPE --> C_MERKLE
    C_ATT --> C_MERKLE
    C_PROVE --> C_FP
    C_PROVE --> C_MERKLE
    C_EMIT --> C_MAN
    C_PUB --> C_EMIT
    C_PUB --> C_ATT
    C_CLI --> C_PIPE
    C_CLI --> C_ATT
    C_CLI --> C_PROVE
    C_CLI --> C_PUB
    C_CLI --> C_EMIT
    C_CLI --> C_LED
    C_CLI --> C_FP
  end

  %% ===================================================================
  %% STYLING
  %% ===================================================================
  classDef protected fill:#fff4e8,stroke:#c0633a,stroke-width:3px,color:#6a1a00
  classDef out fill:#e0eaff,stroke:#3a55c0,stroke-width:2px,color:#1a1a4a
  classDef cli fill:#fde0e6,stroke:#c03a55,stroke-width:2px,color:#5a1a2a

  %% PROTECTED nodes
  class CAS_STAT,CAS_PRESENT,CAS_FAST,CAS_TMP,CAS_FSYNC1,CAS_RENAME,CAS_FSYNC2 protected
  class M_EMPTY,M_EMPTY_OUT,M_LH,M_LEVEL,M_PAIRS,M_ODD,M_CARRY,M_NEXT,M_DONE,M_ROOT,LEAVES protected
  class SCHEMA,WRITER_CFG,KV,RBATCH,PWRITE,MANIFEST protected
  class C_CAS,C_MERKLE,C_MAN protected

  %% Output artifacts
  class BOUT,BUNDLE,EMIT_C,EMIT_CARD,EMIT_V,EMIT_53,EMIT_BOM,HF_RES out
  class V_GREEN,PR_SIGN,TD_PUB,CI_GREEN out

  %% CLI commands
  class OP_CMD,HF_CMD,PR_CMD,TD_CMD cli
```

---

## Numbered narrative — the 18 phases explained

### [1] Operator input

The operator types `attestrum build` with a `--corpus corpus.toml`, a `--workspace` path (where `.attestrum/` lives), and an optional `--source-date-epoch` for the Reproducible Builds timestamp. The TOML lists `[[entry]]` blocks per BUILD-PLAN §8.3, each pointing to a `source_uri` plus a `ContentSource` (`Path` or `Bytes` in v1; HTTP in Sprint 4). The CLI parses this into `Vec<CorpusEntry>` and hands it to `attestrum_pipeline::build_corpus`. **Why it matters**: this is the trust handoff — everything downstream is a deterministic function of `Vec<CorpusEntry>` + the writer config + the PROTECTED hash/merkle/manifest contracts.

### [2] Signal sources fetched per origin

Attestrum doesn't fetch a robots.txt per document; it fetches one per origin and caches. The signal sources are: `robots.txt` (RFC 9309), `ai.txt` (Spawning convention), TDMRep (three resolution layers: well-known JSON → HTTP header override → meta-tag override), IPTC PLUS DMI in XMP sidecars, C2PA training-mining assertions, RSL, Liccium TDM-AI ISCC sidecars, Cloudflare AI Crawl Control headers. **Why it matters**: this is the EU AI Act §53(1)(d) compliance substrate — the manifest's `signals` STRUCT records what was found per document, so an auditor can reconstruct which opt-out signal was honored (or ignored, depending on `--ruleset`).

### [3] Per-signal parsers (attestrum-signals)

Each signal source runs through its own `SignalParser` impl. RobotsParser handles RFC 9309 edge cases (404 → `Unknown`, empty body → `Unknown`, matched group → `Allowed | Disallowed`). TdmrepParser implements the W3C three-layer override resolution. Every parser returns a `SignalVerdict` ∈ `{Allowed, Disallowed, Unknown}` + the raw value for the manifest. **Why it matters**: signal parsing is deterministic, pure (bytes in, verdict out, no network), and fully testable. Proptest on the aggregator (Sprint 2 E2) verifies the rules engine semantics regardless of which (signal × ruleset) combo lands at the input.

### [4] Signal aggregation → SignalDecision

For each document, the aggregator collects per-signal verdicts and applies the operator's `--ruleset`: `strict` rejects any `Disallowed`; `audit-only` flags `Unknown` for human review; `permissive` includes by default and logs. Output is a `SignalDecision { included: bool, exclusion_reason: Option<String>, signals: ManifestSignals }` — the 12-field signals STRUCT lives in this struct and gets embedded verbatim into each `ManifestEntry`. **Why it matters**: this is the only place in the pipeline where policy meets evidence. The Merkle root attests to whatever the ruleset decided — auditors can re-run with a different ruleset and get a different decision; the cryptographic binding is to the bytes + the decision, not to the policy itself.

### [5] Rayon parallel hash + CAS — the per-worker pipeline

`build_corpus` calls `entries.par_iter().enumerate()` and uses Rayon's `fold + reduce` so each worker thread gets its OWN `Vec<ManifestEntry>` accumulator — no `Mutex<Vec>`, no `crossbeam_channel`, no shared mutable state. Per worker: (a) read bytes from `ContentSource`, (b) stream them through BLAKE3 + SHA-256 simultaneously via an 8 KiB tee buffer (never holds the full document in RAM), (c) call `CasStore::put(digest, bytes)` which writes to `.attestrum/tmp/.attestrum-tmp.<pid>-<n>-<nanos>`, fsyncs, then `rename(2)`s atomically to `.attestrum/cas/blake3/<ab>/<cd>/<hex>.bin` and fsyncs the parent directory, (d) build a `ManifestEntry` with `input_ordinal` stamped from the enumerate index. **Why it matters**: this is where determinism is earned. Rayon's work-stealing schedule is non-deterministic, but `input_ordinal` is stamped at construction time, so the final manifest order is recoverable regardless of which thread completed which entry first.

### [6] Single-thread epilogue — sort + bind

After all workers finish, `reduce` merges per-worker `Vec`s via `Vec::append` (O(N) memcpy, single-threaded, deterministic). `sort_by_key(input_ordinal)` restores canonical input order. `assign_occurrence_indices` walks input-order assigning `0, 1, 2, ...` per-digest rank (this is what makes the multiset duplicate-leaf policy auditable — see §[8]). `sort_entries` resorts by `(document_id, occurrence_index)` for the canonical on-disk order. **Why it matters**: the sort key is the contract. The Merkle tree extracts leaves in this order; any auditor who reads the Parquet, sorts the same way, and re-computes the root MUST get the same bytes — that's the byte-identity guarantee tested in §[17].

### [7] PROTECTED Parquet manifest write

The schema is 18 Arrow columns (16 from BUILD-PLAN §4.2 + `input_ordinal` + `occurrence_index`), encoded with strict deterministic Parquet settings: writer version `PARQUET_1_0` (simpler PLAIN+RLE encodings than 2.0's DELTA/BYTE_STREAM_SPLIT/RLE_DICTIONARY), `ZSTD` compression at level 3, dictionary encoding disabled globally (avoids parquet-rs's adaptive fallback heuristic), statistics disabled globally (avoids stats-truncation drift), bloom filters off, `created_by` pinned to `"attestrum-manifest/0.1.0"` (parquet-rs's default embeds the crate version), KeyValue metadata as a sorted `Vec<KeyValue>` (NOT `HashMap` — iteration order is non-deterministic). **Why it matters**: this is the on-disk artifact format. Once shipped (Sprint 3 E3, commit `2ff2d2b`), changing ANY of these settings invalidates every previously-emitted Attestrum manifest. The cross-check at `~/Downloads/attestrum-e3/` is the receipt for these choices.

### [8] PROTECTED RFC 6962 Merkle over BLAKE3

Empty corpus → root = `BLAKE3("")` = `af1349b9...`. Otherwise: each document_id becomes a leaf, hashed as `BLAKE3(0x00 || leaf)` (the `0x00` is RFC 6962's leaf domain separator). Pairs at each level combine via `BLAKE3(0x01 || left || right)` (the `0x01` is the internal-node domain separator). When a level has an odd number of nodes, the lone rightmost is **carried up unchanged** (RFC 6962 rule, NOT Bitcoin's "duplicate and re-hash"). Multiset policy: identical leaves are preserved as adjacent entries, not deduplicated — that's why `(document_id, occurrence_index)` is the sort key, and why `input_ordinal` (from §[6]) lets an auditor verify the binding without trusting Attestrum. **Why it matters**: a wrong byte here invalidates every signed bundle Attestrum has ever issued. PROTECTED per CLAUDE.md §4; shipped Sprint 2 E7+E8.

### [9] BuildOutput

`BuildOutput { merkle_root: [u8; 32], manifest_path: PathBuf, leaf_count: usize, total_bytes: u64 }` is what the pipeline returns. Three artifacts on disk: `manifest.parquet`, `merkle.root` (a single hex line), and the populated `.attestrum/cas/blake3/.../`. **Why it matters**: this is what `attestrum inspect` reads to summarize the build, and what `attestrum-attest` wraps in an in-toto Statement (§[10]).

### [10] in-toto Statement v1 wraps the corpus

`attestrum-attest` builds an in-toto Statement v1: `subject = [{name: "manifest.parquet", digest: {sha256: SHA256(manifest.parquet)}}]`, `predicateType = "https://attestrum.com/attestation/training-corpus/v0.1"`, predicate carries `merkle_root` + aggregations (document_count, modality breakdown, license breakdown, signal coverage, ruleset, source_date_epoch, `builderVersion = "attestrum/x.y.z"`). **Why it matters**: in-toto is the industry-standard wrapper for SLSA-style attestations. Cosign verifies in-toto Statements natively; auditors don't need Attestrum to parse the predicate (it's just JSON).

### [11] DSSE + Sigstore Bundle v0.3

The Statement is base64-encoded as a DSSE payload with `payloadType = "application/vnd.in-toto+json"`. The operator's OIDC id_token (from GitHub Actions, Google, Microsoft, etc.) goes to Fulcio CA, which returns a short-lived X.509 cert + ephemeral key. DSSE signs the payload with the ephemeral key. The DSSE envelope + verification material submits to Rekor v2 (tile-backed), which returns a signed inclusion proof + RFC3161 timestamp. All of it assembles into a `Sigstore Bundle v0.3` JSON with media type `application/vnd.dev.sigstore.bundle.v0.3+json` — `bundle.sigstore.json` is the file. **Why it matters**: Bundle v0.3 is what `cosign v3+ verify-blob-attestation --new-bundle-format` consumes. The whole purpose of this phase is to produce a self-contained artifact that verifies WITHOUT Attestrum installed.

### [12] attestrum-emit sidecar artifacts

`ManifestReader` walks the sealed Parquet and computes aggregations (modality count, size buckets, registered_domain histogram via publicsuffix, license SPDX coverage, signal coverage). From those: Croissant JSON-LD (`schema.org/Dataset` + Attestrum provenance fields), the dataset README.md with YAML frontmatter, `verify.html` (static page with embedded WASM cosign-lite verifier — no JS framework, no network), Article 53 PDF + `summary.json` via Typst (pinned font set for PDF determinism), CycloneDX 1.7 ML-BOM `attestrum.cdx.json`. **Why it matters**: these are the artifacts that make Attestrum output legible to NON-cryptographic auditors — a compliance officer reads the PDF, an ML engineer reads Croissant, a security auditor reads the cosign bundle. Each lands as a separate file in the published repo.

### [13] attestrum publish to Hugging Face Hub

`attestrum publish --target hf --dataset org/name --bundle bundle.sigstore.json` calls `POST /api/repos/create` (idempotent `exist_ok=true`), then `create_commit` with six operations: `README.md`, `croissant.json`, `attestrum/manifest.parquet`, `attestrum/merkle.root`, `attestrum/bundle.sigstore.json`, `attestrum/verify.html`. Returns the dataset URL + a webhook notification to downstream consumers. **Why it matters**: HF Hub doesn't have native Sigstore attestation support for datasets (verified May 2026), so the bundle ships as a regular repo file — exactly like the OpenSSF model-signing project does for models. The verify.html is the visitor-facing UI; the manifest.parquet is the auditable substrate.

### [14] Third-party verify (NO Attestrum install)

This is the Sprint 6 demo + the Layer 4 wedge test. A fresh visitor with NOTHING but a browser opens `verify.html`. The embedded WASM cosign-lite fetches the bundle + manifest, refreshes TrustRoot via TUF, verifies the Fulcio cert chain, verifies the OIDC identity claim (`--certificate-identity-regexp`), verifies the Rekor inclusion proof + RFC3161 timestamp. All-OK → green check + "corpus authentic." Any failure → red X with a diagnostic. **Why it matters**: this is the entire "substrate not silo" claim from CLAUDE.md §12. If cosign-lite alone can't verify, every acquirer-optionality bet collapses. Sprint 6 acceptance criterion is exactly this flow on a fresh laptop.

### [15] attestrum prove — inclusion or non-inclusion

The HEADLINE Path A command. Operator runs `attestrum prove DOC --against MANIFEST`. Input can be a BLAKE3 hex (exact match), an ISCC URI (similarity), a perceptual hash (distance), or a raw document (run through `attestrum-fingerprint` first: text gets MinHash+SimHash+ISCC, image gets pHash+blockhash+ISCC, audio gets chromaprint+ISCC, video gets keyframe pHash+ISCC). Manifest loads from local Parquet, `hf://org/name`, or a registry. If match: build the Merkle audit path (sibling chain from leaf_index to root), verify it with `verify_audit_path` (replays the level walk), wrap in `inclusion-proof/v0.1` predicate. If no match: build a sorted-neighbor non-inclusion proof, wrap in `non-inclusion-proof/v0.1`. Either way, sign as a SEPARATE DSSE + Sigstore bundle. **Why it matters**: this is the auditor's tool. A rightsholder asking "is my work in your corpus?" gets a cryptographically signed yes-or-no with the actual leaf bytes + audit path. No-Attestrum verification works (the predicate is just JSON, the audit path is just an array of 32-byte hashes).

### [16] attestrum takedown

Rightsholder submits a request `{doc_hash, reason, evidence}`. `attestrum-ledger` verifies standing, appends a takedown leaf to a local append-only log. Witness mode (operator choice): submit to Rekor v2 (predicate `takedown/v0.1`), append to an HF Hub witness repo (`org/dataset-witness/log.jsonl`, contractually equivalent to a tiled append-only log), or local-only. A NEW corpus version is computed with `prev_root = v_n.merkle_root` (cryptographic chain), re-signed with `training-corpus/v0.1`, republished to HF. **Why it matters**: this is the right-to-be-forgotten path. The takedown leaf is permanent (you can't erase the fact that takedown happened); the corpus is materially different (the document is no longer in the new manifest). Auditors walk the chain `v_n → v_n+1 → ...` to reconstruct the takedown history.

### [17] CI determinism matrix

`.github/workflows/determinism.yml` runs on 4 targets: `linux-x86_64-glibc` (ubuntu-24.04), `linux-aarch64-glibc` (ubuntu-24.04-arm), `macos-aarch64-darwin` (macos-14), `linux-x86_64-musl` (alpine:3.20 container). Each: `cargo test --workspace` + run the Sprint 2 corpus example + (Sprint 3 E8) run `attestrum build` on `synthetic-1k.toml`. Each uploads `merkle-root-<label>.txt` + `manifest.parquet`. The `compare` job `cmp -s`s pairwise across all 4 targets. Any divergence → CI red with `::error::` annotation at the divergent byte offset. **Why it matters**: this is the mechanical test for "works." If the matrix is green, the determinism CONTRACT is upheld — same input on any of the 4 targets produces byte-identical output. Local in-process tests can lie about cross-platform behavior; this can't.

### [18] Workspace dep graph (14 crates)

The architecture in one diagram. `attestrum-core` is the substrate (every other crate depends on it). The PROTECTED triangle is `attestrum-cas + attestrum-merkle + attestrum-manifest`. `attestrum-pipeline` wires those three plus `attestrum-signals` into the build. `attestrum-attest` + `attestrum-emit` + `attestrum-publish` + `attestrum-prove` + `attestrum-fingerprint` + `attestrum-ledger` sit one layer up. `attestrum-cli` consumes everything and provides the user-facing commands. `attestrum-fingerprint-registry` is the v1+ optional shared registry. **Why it matters**: a clean dep graph makes the project legibly modular. Reverse-deps from the PROTECTED layer (anything that depends on `attestrum-merkle` or `attestrum-manifest::io::*`) are the things a PROTECTED-system change would break.

---

## Cross-phase data flow — the 6 most important inter-subgraph arrows

These are the spine of the whole pipeline. Walk these arrows to trace the build end-to-end.

| From → To | What flows | Why it matters |
|---|---|---|
| `CE_LIST` (P1) → `PAR` (P5) | Vec CorpusEntry → per-worker hashing | Operator input becomes a parallel work stream |
| `DEC_FINAL` (P4) → `ROW` (P5) | SignalDecision per doc → ManifestEntry construction | Policy decision becomes part of the row |
| `SORT_CANON` (P6) → `LEAVES` (P8) AND → `RBATCH` (P7) | Sorted entries → both Merkle + Parquet | The single sort defines BOTH the on-disk row order AND the Merkle leaf order |
| `MANIFEST` (P7) → `SH_MAN` (P10) | manifest.parquet → SHA-256 → subject of in-toto Statement | The Parquet file IS what gets signed |
| `BUNDLE` (P11) → `HF_O5` (P13) | bundle.sigstore.json → HF Hub repo file | The signature ships alongside the corpus |
| `V_GREEN` (P14) | (terminal node) | The whole pipeline exists to make this node reachable for a fresh visitor with no Attestrum install |

---

## How to use this map

- **First time reader**: Start at `[1]`. Walk down through `[6]` for the build. Skip to `[14]` to see what an outside party experiences. Then come back to `[10]–[13]` to fill in how the signing+publish works.
- **PROTECTED-system change**: Look at orange-bordered nodes. They cluster in P5 (CAS), P7 (manifest), P8 (Merkle). Any change to these nodes requires CLAUDE.md §4's `Protected-system-change:` commit footer + bumping `SCHEMA_VERSION` / `WRITER_PROFILE` where applicable.
- **Determinism debugging**: Walk backwards from P9 (`BuildOutput`) through P8 (Merkle) → P7 (Parquet) → P6 (sort) → P5 (hash+CAS) → P1 (input). The byte you're trying to explain has to flow through every one of these in order.
- **Planning a new feature**: Find where it'd insert into the chain. Most additions go in P5 (per-doc transformations), P10–P12 (new predicate types or sidecar formats), or P15 (new prove types).
- **Acquirer due diligence**: P14 + P18. P14 proves the artifact stands on its own; P18 proves the architecture is modular (no Attestrum-flavored lock-in points).

---

## File map

| File | Path | What |
|---|---|---|
| This file | `/Users/austinmunday/Documents/Claude/Attestrum/ANNEX-EVERYTHING.md` | The everything map (source) |
| Desktop copy | `~/Desktop/attestrum-everything.md` | Identical copy |
| Desktop PNG | `~/Desktop/attestrum-everything.png` | Rendered at scale=3 — open in Preview, fit-window, click around |
| Companion: meta-map | `/Users/austinmunday/Documents/Claude/Attestrum/DIAGRAMS-OVERVIEW.md` | Earlier "diagrams as nodes" view (27 boxes, not 140) |
| Source diagrams | `/Users/austinmunday/Documents/Claude/Attestrum/docs/diagrams/{overview,sprint-1,sprint-2,sprint-3,attestations}/*.md` | The 27 per-area contracts |
| Live dashboard | `http://127.0.0.1:8766/` (main) + `/diagrams` (catalog) | Live stats + per-diagram rendered view |
