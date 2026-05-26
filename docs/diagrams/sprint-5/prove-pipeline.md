---
title: "Sprint 5 attestrum-prove pipeline — exact + fuzzy match (E1-E5) + non-inclusion (E6) + alternate manifest sources (E7) + CLI + API freeze (E8)"
models: "crates/attestrum-prove/src/lib.rs, crates/attestrum-prove/Cargo.toml, crates/attestrum-fingerprint/src/lib.rs, crates/attestrum-merkle/src/lib.rs, crates/attestrum-manifest/src/lib.rs, crates/attestrum-attest/src/predicate.rs, crates/attestrum-attest/src/sign.rs, crates/attestrum-cli/src/commands/prove.rs, prove, ProofTarget, ManifestSource, ProveOpts, ProofArtifact, ProofKind, AttestrumProveError, InclusionProofPredicate, NonInclusionProofPredicate, MatchEvidence, IsccEvidence, PerceptualEvidence, MinHashEvidence, CorpusRef, MerkleTree, audit_path, FingerprintBundle, fingerprint_text, fingerprint_image, ManifestEntry"
source_of_truth: diagram
last_verified: d122c64 2026-05-26
diagram_type: flowchart
---

# Sprint 5 `attestrum-prove` pipeline

Source of truth: **`diagram`** through S5-D2 E1-E7. This diagram is the contract `attestrum-prove` implements; it flips to `source_of_truth: code` at S5-D2 E8 (the CLI + API freeze commit, mirroring the D1 E5 cadence). The `attestrum-attest` predicate types (`InclusionProofPredicate`, `NonInclusionProofPredicate`, `MatchEvidence`) are already-locked PROTECTED schemas per CLAUDE.md §4 — this diagram references them but does not redefine them.

**This is the ONLY sprint-5 diagram for S5-D2** per the D1 cadence precedent (one diagram per deliverable; per-E-commit updates bump `last_verified` + flip branch nodes from grey-deferred to green-shipped rather than creating a new diagram per commit).

**Branch state at E2** (this commit, exact-match path live, audit-path stubbed): `prove()` now resolves the exact-hash dispatch arms (`ProofTarget::Blake3`, `Sha256`, `Bundle` against `ManifestSource::Local`) end-to-end — reads the local Parquet manifest via `attestrum_manifest::read_manifest`, finds the matching leaf, recomputes the RFC 6962 BLAKE3 Merkle root via `attestrum_merkle::merkle_root`, builds an `InclusionProofPredicate` (with `audit_path: vec![]` placeholder), wraps it in an `InTotoStatement` at predicate type `attestrum.com/attestation/inclusion-proof/v0.3`, and returns an unsigned `ProofArtifact { kind: Inclusion, confidence: 1.0, bundle_path: None, ... }`. The Mermaid nodes that flip from grey to green at this commit: `target`, `manifest`, `opts`, `dispatch`, `resolve`, `exactB3`, `exactS256`, `bundleExact`, `localPq`, `loadIdx`, `matchQuery`, `matchDec`, `predIncl`, `evidenceVariant`, `evExact`, `stmt`, `signCheck`, `unsignedOut`, plus the two now-reachable error edges `errMan` (manifest parse failure) and `errAmb` (multiple leaves with same digest). `auditPath` stays grey because its body is stubbed (`vec![]`) — E3 lands the real audit-path via `attestrum_merkle::MerkleTree::audit_path` and flips that node. Fuzzy paths (`isccPath`, `perceptPath`, `docPath`, `docMulti*`, `fuzzyScan`, `fuzzyDec`, `evIscc`, `evPercept`, `evMinhash`) panic with `unimplemented!("S5-D2 E5+")` pending E5. HF / URL fetches (`hfFetch`, `urlFetch`) panic with `unimplemented!("S5-D2 E7")` pending E7. The non-inclusion path (`nonInc`, `predNonIncl`, `stmtNI`) panics with `unimplemented!("S5-D2 E6")` pending E6. DSSE-sign + signed-output (`dsseSign`, `signedOut`) and CLI (`cliEntry`, `cliPrint`) stay grey pending E4 and E8.

**Parent overview**: `docs/diagrams/overview/prove-pipeline.md` carries the architectural-overview view (sourced from PATH-A-BRIEF Part 1.3 verbatim). This sprint-5 diagram is the implementation-detail view: specific Rust function calls, internal helpers, error edges to typed variants, and the per-E-commit progress tracker. Different audiences.

**PROTECTED dependencies** (CLAUDE.md §4 — `attestrum-prove` consumes these, never modifies them):

1. **`attestrum-attest::predicate::{InclusionProofPredicate, NonInclusionProofPredicate, MatchEvidence}`** — Frozen at v0.3 per the predicate URIs (`attestrum.com/attestation/{inclusion,non-inclusion}-proof/v0.3`). Any schema change requires a v0.4 URI bump + migration packet.
2. **`attestrum-merkle::{MerkleTree, audit_path, verify_audit_path}`** — RFC 6962 binary Merkle over BLAKE3. Audit-path layer landed at Sprint 2 E8.
3. **`attestrum-fingerprint`** — Frozen at v0.1 as of S5-D1 E5 (just landed). `FingerprintBundle` shape, `FINGERPRINT_SCHEMA` URI, normalization pipelines.
4. **PROTECTED extension required at E6 (non-inclusion proof)**: `attestrum-merkle` does not yet have a sorted-Merkle build / adjacent-pair lookup helper. Adding one is a PROTECTED-system extension requiring explicit founder approval in the E6 commit footer.

```mermaid
flowchart TB
  classDef shipped fill:#1f6f3f,stroke:#3ec072,color:#fff
  classDef deferred fill:#3a3a3a,stroke:#666,color:#aaa
  classDef protected fill:#7a1f1f,stroke:#c63737,color:#fff
  classDef output fill:#1a3a6f,stroke:#3a8ed7,color:#fff
  classDef external fill:#5a4a1f,stroke:#a8902f,color:#fff

  subgraph inputs["Caller inputs (E1)"]
    target["target: ProofTarget"]
    manifest["manifest: ManifestSource"]
    opts["opts: ProveOpts"]
  end

  target --> dispatch{"ProofTarget variant?"}

  dispatch -->|"Blake3 raw 32 bytes (E2)"| exactB3["exact-BLAKE3 path"]
  dispatch -->|"Sha256 raw 32 bytes (E2)"| exactS256["exact-SHA-256 path"]
  dispatch -->|"Bundle FingerprintBundle (E2)"| bundleExact["extract blake3+sha256 from bundle"]
  dispatch -->|"Iscc String (E5)"| isccPath["ISCC composite-distance path"]
  dispatch -->|"Perceptual PerceptualHashes (E5)"| perceptPath["perceptual Hamming path"]
  dispatch -->|"Document PathBuf (E5)"| docPath["run fingerprint_text/_image inline"]

  bundleExact --> exactB3

  docPath --> fpDispatch{"file MIME / extension"}
  fpDispatch -->|"text/*"| fpText["fingerprint_text bytes opts"]
  fpDispatch -->|"image/*"| fpImage["fingerprint_image bytes opts"]
  fpDispatch -->|"other"| fpErr["AttestrumProveError::Fingerprint<br/>(modality not yet implemented)"]
  fpText --> docMulti["multi-mode: exact + ISCC + MinHash"]
  fpImage --> docMulti2["multi-mode: exact + ISCC + perceptual"]

  manifest --> resolve{"ManifestSource variant?"}
  resolve -->|"Local PathBuf (E2)"| localPq["mmap Parquet via attestrum-manifest"]
  resolve -->|"HuggingFace repo,revision (E7)"| hfFetch["hf-hub fetch + local cache"]
  resolve -->|"Url url::Url (E7)"| urlFetch["url::Url fetch + cache"]

  hfFetch --> localPq
  urlFetch --> localPq

  localPq --> loadIdx["build query index<br/>BTreeMap blake3 → leaf_index"]

  exactB3 --> matchQuery["query index for exact-hash"]
  exactS256 --> matchQuery
  isccPath --> fuzzyScan["scan all leaves<br/>compute composite distance"]
  perceptPath --> fuzzyScan
  docMulti --> matchQuery
  docMulti --> fuzzyScan
  docMulti2 --> matchQuery
  docMulti2 --> fuzzyScan

  loadIdx --> matchQuery
  loadIdx --> fuzzyScan

  matchQuery --> matchDec{"match found?"}
  fuzzyScan --> fuzzyDec{"distance ≤ threshold?"}

  matchDec -->|"yes"| auditPath["audit_path leaf_index<br/>via MerkleTree (Sprint 2 E8)"]
  matchDec -->|"no"| nonInc["sorted-Merkle adjacent-leaves<br/>build NonInclusionProofPredicate (E6)"]
  fuzzyDec -->|"yes"| auditPath
  fuzzyDec -->|"no, but multi-mode has other matches"| matchQuery
  fuzzyDec -->|"no, all modes failed"| nonInc

  auditPath --> predIncl["build InclusionProofPredicate<br/>with MatchEvidence variant"]
  predIncl --> evidenceVariant{"MatchEvidence kind"}
  evidenceVariant -->|"exact"| evExact["ExactBlake3 / ExactSha256<br/>confidence 1.00 (E3, E4)"]
  evidenceVariant -->|"iscc"| evIscc["Iscc compositeDistance<br/>confidence 0.95 (E5)"]
  evidenceVariant -->|"perceptual"| evPercept["Perceptual hammingDistance threshold<br/>confidence 0.85 (E5)"]
  evidenceVariant -->|"minhash"| evMinhash["MinHash jaccard ngramSize<br/>confidence 0.80 (E5)"]

  nonInc --> predNonIncl["build NonInclusionProofPredicate<br/>sorted-Merkle proof<br/>confidence 1.00 (E6)"]

  evExact --> stmt["wrap in InTotoStatement v1<br/>predicateType=attestrum.com/.../inclusion-proof/v0.3"]
  evIscc --> stmt
  evPercept --> stmt
  evMinhash --> stmt
  predNonIncl --> stmtNI["wrap in InTotoStatement v1<br/>predicateType=attestrum.com/.../non-inclusion-proof/v0.3"]

  stmt --> signCheck{"opts.sign?"}
  stmtNI --> signCheck
  signCheck -->|"yes"| dsseSign["DSSE-sign via attestrum_attest::sign<br/>(Sigstore Bundle v0.3 + Rekor v1 dsse@0.0.1) (E4)"]
  signCheck -->|"no, --unsigned"| unsignedOut["ProofArtifact bundle=None (E1+)"]
  dsseSign --> signedOut["ProofArtifact bundle=Some(Bundle) (E4)"]

  unsignedOut --> result["ProofArtifact { kind, statement,<br/>bundle, confidence, matched_subject }"]
  signedOut --> result

  cliEntry["attestrum prove DOC --against MANIFEST<br/>(CLI subcommand, E8)"] --> target
  result --> cliPrint["print confidence + bundle_path + Exit 0 (E8)"]

  errFp["AttestrumProveError::Fingerprint<br/>#[from] AttestrumFingerprintError"]
  errSrc["AttestrumProveError::SourceUnreachable"]
  errMan["AttestrumProveError::InvalidManifest"]
  errMerk["AttestrumProveError::MerkleMismatch"]
  errSign["AttestrumProveError::Sign<br/>#[from] AttestrumAttestError"]
  errAmb["AttestrumProveError::Ambiguous(usize)"]

  fpErr -. "Fingerprint" .-> errFp
  hfFetch -. "SourceUnreachable" .-> errSrc
  urlFetch -. "SourceUnreachable" .-> errSrc
  localPq -. "InvalidManifest" .-> errMan
  auditPath -. "MerkleMismatch" .-> errMerk
  dsseSign -. "Sign" .-> errSign
  matchQuery -. ">1 leaf with same digest" .-> errAmb

  protectedAttest["PROTECTED — attestrum-attest predicates<br/>InclusionProofPredicate, NonInclusionProofPredicate,<br/>MatchEvidence, CorpusRef<br/>(v0.3 URIs, schema-frozen)"]
  protectedMerkle["PROTECTED — attestrum-merkle<br/>MerkleTree, audit_path, verify_audit_path<br/>(RFC 6962 + BLAKE3)"]
  protectedFp["PROTECTED — attestrum-fingerprint v0.1<br/>FingerprintBundle, FINGERPRINT_SCHEMA,<br/>normalization pipelines"]

  predIncl -.-> protectedAttest
  predNonIncl -.-> protectedAttest
  auditPath -.-> protectedMerkle
  fpText -.-> protectedFp
  fpImage -.-> protectedFp

  class target,manifest,opts,dispatch,resolve,exactB3,exactS256,bundleExact,localPq,loadIdx,matchQuery,matchDec,predIncl,evidenceVariant,evExact,stmt,signCheck,unsignedOut,errMan,errAmb shipped
  class isccPath,perceptPath,docPath,fpDispatch,fpText,fpImage,fpErr,docMulti,docMulti2,hfFetch,urlFetch,fuzzyScan,fuzzyDec,auditPath,nonInc,evIscc,evPercept,evMinhash,predNonIncl,stmtNI,dsseSign,signedOut,cliEntry,cliPrint,errFp,errSrc,errMerk,errSign deferred
  class protectedAttest,protectedMerkle,protectedFp protected
  class result output
```

**Legend**:

- **Grey nodes** (`deferred`): not yet shipped. Each E-commit flips its sub-graph from grey to green. Pre-E1 state: everything grey.
- **Green nodes** (`shipped`): land in or before the current commit.
- **Red nodes** (`protected`): PROTECTED dependencies per CLAUDE.md §4. Consumed but never modified by `attestrum-prove`.
- **Blue nodes** (`output`): user-facing returned values.

## What lands at Sprint 5 D2 E1 — scaffold + types (proposed)

- `crates/attestrum-prove/Cargo.toml` — fills the currently-empty `[dependencies]` block with the MINIMAL set for E1 (per CLAUDE.md §14 "Eager generalization" anti-pattern; each subsequent E-commit adds its own deps):
  - `attestrum-core = { path = "..." }` — for `Modality` + `AttestrumError`
  - `attestrum-attest = { path = "..." }` — for `InclusionProofPredicate`, `MatchEvidence`, `AttestrumAttestError` (the `#[from]` source for `AttestrumProveError::Sign`)
  - `attestrum-fingerprint = { path = "..." }` — for `FingerprintBundle` (carried in `ProofTarget::Bundle`) + `AttestrumFingerprintError` (the `#[from]` source for `AttestrumProveError::Fingerprint`)
  - `serde = { workspace = true }` — derive macros on the public types
  - `thiserror = { workspace = true }` — `AttestrumProveError`
  - Deferred to later E-commits: `parquet`, `arrow`, `attestrum-manifest`, `attestrum-merkle` (E2), `hf-hub`, `url` (E7).
- `crates/attestrum-prove/src/lib.rs` — replaces the 1-line stub with the public API surface:
  - `pub enum ProofTarget { Blake3([u8;32]), Sha256([u8;32]), Iscc(String), Perceptual(PerceptualHashes), Document(PathBuf), Bundle(FingerprintBundle) }` — six variants per PATH-A-BRIEF §2.2 (one delta from the brief: explicit `Sha256` variant since the predicate's `ExactSha256` evidence implies the caller needs to be able to drive it independently of BLAKE3).
  - `pub struct PerceptualHashes { pub phash: [u8;8], pub blockhash: [u8;8] }` — caller-supplied 64-bit perceptual hashes for the non-fingerprint-inline path.
  - `pub enum ManifestSource { Local(PathBuf), HuggingFace { repo: String, revision: Option<String> }, Url(url::Url) }` — three variants per the brief. `url::Url` deferred behind a feature gate or replaced with `String` at E1 since `url` isn't yet a dep — TBD by founder review.
  - `pub struct ProveOpts { pub sign: bool, pub source_date_epoch: i64, pub oidc_id_token: Option<String>, pub workspace: Option<PathBuf> }` — `sign=true` is default; `--unsigned` flag at E8 flips it.
  - `pub struct ProofArtifact { pub kind: ProofKind, pub statement: InTotoStatement, pub bundle: Option<Bundle>, pub confidence: f32, pub matched_subject: Option<Subject> }` — `Bundle` re-exported from `attestrum-attest`.
  - `pub enum ProofKind { Inclusion, NonInclusion }`.
  - `pub enum AttestrumProveError` — six variants from the brief: `SourceUnreachable`, `InvalidManifest`, `MerkleMismatch`, `Fingerprint(#[from] AttestrumFingerprintError)`, `Sign(#[from] AttestrumAttestError)`, `Ambiguous(usize)`.
  - `pub fn prove(target: ProofTarget, manifest: ManifestSource, opts: &ProveOpts) -> Result<ProofArtifact, AttestrumProveError>` — body is `unimplemented!("S5-D2 E2+ fills this in")`. Pre-locks the contract.
- Inline `#[cfg(test)]` tests: type-construction smoke tests (each `ProofTarget` variant constructs without panic; `AttestrumProveError` round-trips via Debug).
- This diagram file (`docs/diagrams/sprint-5/prove-pipeline.md`) — `last_verified` SHA bump to the E1 commit. No structural changes to the Mermaid; only the legend's "Branch state at E…" header updates.
- `CHANGELOG.md` `[Unreleased]` — new `### Added — Sprint 5` sub-bullet for D2 E1 (scaffolding only — no user-facing prove() yet).

## What lands at Sprint 5 D2 E2-E8 (one-line each)

- **E2**: local-Parquet manifest read + exact-BLAKE3/SHA-256 match path + `InclusionProofPredicate` emission WITHOUT audit-path (placeholder `audit_path: vec![]`) and WITHOUT signing (`opts.sign=false` forced). First end-to-end shape.
- **E3**: audit-path via `attestrum-merkle::MerkleTree::audit_path(leaf_index)`. Predicate now carries a real proof; verifier can recompute the root.
- **E4**: DSSE-sign via `attestrum-attest::sign`. **MVP gate** — first demonstrable signed inclusion proof verifiable end-to-end via `cosign v3+ verify-blob-attestation --new-bundle-format`.
- **E5**: fuzzy-match paths (`Iscc`, `Perceptual`, `MinHash`) with confidence reporting per the PATH-A-BRIEF §2.2 thresholds. `ProofTarget::Document` runs `attestrum-fingerprint` inline.
- **E6**: `NonInclusionProofPredicate` via sorted-Merkle adjacent-leaves. **Requires a PROTECTED-system extension to `attestrum-merkle`** (sorted-tree build + adjacent-pair lookup helpers); requires explicit founder approval in the commit footer.
- **E7**: alternate manifest sources (`HuggingFace`, `Url`) with caching. May depend on `attestrum-publish` (Sprint 5 D3) for HF Hub auth patterns — coordinate.
- **E8**: CLI subcommand `attestrum prove DOC --against MANIFEST` + hand-rolled `tests/api_surface.rs` (mirroring D1 E5 precedent) + `source_of_truth: diagram → code` flip on this file. **v0.1 release-ready** after E8.

## What's NOT in scope yet

- **Proof verification CLI** — `attestrum verify <bundle>` already covers single-bundle verification (Sprint 4 E4); a separate `attestrum verify-inclusion <proof-bundle> --against <manifest>` may land in Sprint 6 or be deferred to v0.2.
- **Multi-document batch prove** — given a list of documents, emit one inclusion-or-non-inclusion proof per document. Useful for "I want to attest this entire derivative corpus is a subset of that source corpus"; v2 territory.
- **Threshold-tuning knobs** — exposing the `ISCC ≤ 4` / `Perceptual ≤ 6` / `MinHash ≥ 0.85` thresholds as caller-tunable options. v0.1 hardcodes the PATH-A-BRIEF §2.2 values; tuning lands when a publisher actually asks for it.
- **Witness federation** — submitting non-inclusion-proof entries to Rekor or HF for third-party witness signatures. v0.2 — out of Sprint 5 entirely.

## Design notes carrying forward to commit time

1. **E1 dep minimization** — only the deps actually consumed at E1 are in Cargo.toml. Per CLAUDE.md §14 "Eager generalization" anti-pattern, each later E adds its own deps.
2. **`ManifestSource::Url` URL type** — using `url::Url` requires the `url` crate at E1 even though no E1 code parses URLs. **Recommendation**: use `pub url: String` at E1 (carry the string verbatim); promote to `url::Url` at E7 when fetching code lands. Avoids a phantom dep.
3. **Trait abstraction over `ManifestSource`** — DO NOT introduce `trait ManifestReader` at E1. Match-based dispatch in `prove()` is fine for two-three concrete variants; extract a trait at E7 when the second source (HF) goes live and the duplication is concrete.
4. **`E5` fuzzy-match thresholds** — hardcode the §2.2 table values (`Iscc ≤ 4`, `Perceptual ≤ 6`, `MinHash ≥ 850_000` ppm). No exposed knobs in v0.1; v0.2 considers `ProveOpts` extension.
5. **`E6` sorted-Merkle PROTECTED extension** — adding sorted-tree build + adjacent-pair lookup to `attestrum-merkle` is a PROTECTED-system extension. The commit footer must carry `Protected-system-change: approved-by=Austin on=YYYY-MM-DD` per CLAUDE.md §4.
6. **`E7` HF auth** — coordinate with Sprint 5 D3 (`attestrum-publish`) on the HF Hub token-handling patterns. May land D3 before D2 E7 to avoid duplicating auth logic.
