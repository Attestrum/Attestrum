---
title: "attestrum sign flow — DSSE-wrapped Sigstore Bundle v0.3 emission (Session 1 contract for X→Y hybrid)"
models: "crates/attestrum-attest/src/sign.rs, crates/attestrum-attest/src/dsse_sign.rs, crates/attestrum-attest/src/statement.rs, crates/attestrum-attest/src/predicate.rs, crates/attestrum-cli/src/commands/sign.rs, crates/attestrum-cli/src/lifecycle.rs, crates/attestrum-cli/tests/sign_flow_contract.rs, attest_sign, dsse_sign, sign_dsse, statement, predicate, lifecycle, TRAINING_CORPUS_PREDICATE_TYPE, sigstore::bundle::sign::SigningContext, sigstore::bundle::sign::SigningSession, in-toto Statement v1, DSSE Envelope, application/vnd.in-toto+json, application/vnd.dev.sigstore.bundle+json;version=0.3, DSSERequestV002"
source_of_truth: diagram
last_verified: bootstrap 2026-05-25
diagram_type: sequenceDiagram
---

# `attestrum sign` flow — DSSE-wrapped Sigstore Bundle v0.3 sign half

Source of truth: `diagram` (Session 1 contract for the X→Y hybrid cosign-interop fix, per [`attestrum-cosign-interop-verification-report-2026-05-25.md`](../../../attestrum-cosign-interop-verification-report-2026-05-25.md) §10 + [`attestrum-cosign-interop-decision-2026-05-25.md`](../../../attestrum-cosign-interop-decision-2026-05-25.md) §6). The Mermaid block below describes the **contracted** DSSE-wrapped sign flow that Session 2 will implement in `crates/attestrum-attest/src/dsse_sign.rs`; the historical "What landed at Sprint 4 E3.5" section near the foot of this document describes the **shipped-but-wrong** flow this contract replaces. Source-of-truth flips back to `code` after Session 4 confirms `.github/workflows/cosign-interop.yml` is green against the implemented module.

**Session 1 contract delta (2026-05-25)** — diagnosis verified V1-V7: sigstore-rs 0.14.0's `SigningSession::sign(input)` is a blob-signing primitive. It SHA-256-hashes `input`, signs the hash with the ephemeral ECDSA-P256 key, submits a `Hashedrekord` Rekor v1 entry, and returns a `SigningArtifact` whose `to_bundle()` hard-codes `Bundle v0.2` mediaType + `Content::MessageSignature` (see `bundle/sign.rs:351-382` in sigstore-rs at fork rev `ade5422`). Attestrum's `attest_sign` passed the canonical in-toto Statement JSON bytes as `input`, so the on-disk bundle's signature was over `SHA-256(canonical Statement JSON)`. `attest_verify` then asked sigstore-rs to verify against `SHA-256(manifest.parquet)`. Different byte streams → ring rejects → `PublicKeyVerificationError`. The new flow below signs **DSSE PAE bytes** and emits **Bundle v0.3** with `dsseEnvelope` content + a **`dsse`-kind Rekor v2 entry** (`DSSERequestV002`), corresponding to what `cosign verify-blob-attestation --new-bundle-format` actually accepts.

**Removed vs the prior wrong-shape diagram**:

- `SigningSession::sign(input)` call (blob-signing primitive) → `SigningSession::sign_dsse(payload_type, payload)` (new fork API, Session 5 lands it; Session 2 calls via `[patch.crates-io]`).
- `Hashedrekord` Rekor v1 entry → `DSSERequestV002` Rekor v2 entry, **kind = `dsse`** (not `intoto` per V2 correction of Reviewer 1's hedge — sigstore-go's `pkg/sign/transparency.go` switch picks `dsse` for DSSE envelopes).
- `Content::MessageSignature` bundle content → `Content::DsseEnvelope` content with `payloadType = "application/vnd.in-toto+json"`.
- Bundle v0.2 mediaType (`application/vnd.dev.sigstore.bundle+json;version=0.2`) → Bundle v0.3 mediaType (`application/vnd.dev.sigstore.bundle+json;version=0.3`).
- `input_digest` field (SHA-256 of the input bytes) → removed; DSSE signs PAE bytes directly, not the input's digest.

**Fork touchpoint** — Session 5 introduces a single new method on `impl SigningSession`:

```rust
pub async fn sign_dsse(self, payload_type: &str, payload: &[u8]) -> SigstoreResult<SigningArtifact>
```

…landed as a third commit on the existing `Attestrum/sigstore-rs` fork's `attestrum/email-optional-for-workload-identity-tokens` branch. The method calls `SigningSession::materials()` internally — the fork's empty-emailAddress workload-identity patch at sigstore-rs `bundle/sign.rs:97-110` stays load-bearing under this code path (V5 verified). Session 2's `crates/attestrum-attest/src/dsse_sign.rs` consumes the fork-method via `[patch.crates-io]`. Session 6 (out-of-band, async, multi-week) upstreams the fork-method as a sigstore-rs PR.

**DSSE PAE byte-length semantics** (per verification report §10):

```
PAE = "DSSEv1 " + len(payload_type) + " " + payload_type + " " + len(payload) + " " + payload
```

where `len(x)` is the **UTF-8 byte length** of `x` (= `x.len()` for `&str`/`&[u8]` in Rust), NOT the character count. Spec test vectors at `https://github.com/secure-systems-lab/dsse/blob/master/protocol.md` go into Session 2's unit tests (K3 mitigation per decision doc §4).

**Contract-test obligation closed at `crates/attestrum-cli/tests/sign_flow_contract.rs`** (Sprint 4 E3.5). The contract test enumerates `sign_documented_transitions()` (22 edges) plus four proptest properties (documented-edges-reachable, undocumented-holds, paths-terminate-in-known-exit, exit-codes-in-allowed-set) plus two end-to-end smokes (`--offline` → exit 3 + no bundle written; missing manifest → exit 2 before offline / OIDC dispatch). The state machine still describes subcommand-level transitions accurately; only the internal sign-mechanism changes under the X→Y hybrid.

**Subcommand contract** (unchanged): `attestrum sign <manifest>` takes a sealed `manifest.parquet` (PROTECTED schema per Sprint 3 E3), optionally a `--workspace <dir>` (default: `.attestrum/` in CWD), optional `--source-date-epoch <secs>` for builder-version-fields determinism, and OIDC configuration (env vars `SIGSTORE_ID_TOKEN` / `SIGSTORE_OIDC_ISSUER` / `SIGSTORE_OIDC_CLIENT_ID`, or `--oidc-flow {interactive,workload,token-file}` flag with corresponding sub-flags). Writes the bundle to `<workspace>/bundles/manifest.sigstore.json`. Always networks (Fulcio + Rekor + optional OIDC IdP); `--offline` exits 3.

**Exit codes** (per PATH-A-BRIEF §5.2): `0` ok; `1` runtime error (manifest read, JSON serialize, file I/O on bundle write); `2` clap parse / arg-validation error; `3` `--offline` violation; `4` signing identity error (OIDC token fetch failed, Fulcio rejected CSR, certificate validity window not in future at issuance time); `5` network error (Fulcio 5xx, Rekor 5xx, TUF refresh failed); `8` predicate JSON-Schema validation failure (the assembled `TrainingCorpusPredicate` does not satisfy the v0.1 schema — guard against drift between Rust types and the published schema).

```mermaid
sequenceDiagram
  autonumber
  participant U as User CLI<br/>attestrum sign manifest.parquet
  participant Cmd as attestrum_cli::commands::sign::run
  participant Att as attestrum_attest::sign::attest_sign<br/>(wrapper, delegates to dsse_sign)
  participant Dse as attestrum_attest::dsse_sign<br/>(NEW — Session 2)
  participant Mn as attestrum_manifest::read_manifest
  participant Pred as attestrum_attest::predicate::TrainingCorpusPredicate
  participant Stmt as attestrum_attest::statement::InTotoStatement
  participant Schema as schemars JSON-Schema validate
  participant Oidc as sigstore::oauth::IdentityToken
  participant Ctx as sigstore::bundle::sign::SigningContext
  participant Sess as SigningSession::sign_dsse<br/>(fork API — Session 5)
  participant Fulcio as Fulcio v2 signingCert<br/>(called inside sign_dsse via materials)
  participant Rekor as Rekor v2 DSSERequestV002<br/>(kind=dsse, version=0.0.2)
  participant Bun as Sigstore Bundle v0.3<br/>(mediaType per prose above)
  participant Fs as workspace/bundles/manifest.sigstore.json

  U->>Cmd: parse args (manifest, workspace, oidc_flow, source_date_epoch)
  Cmd->>Cmd: validate args, resolve workspace, locate manifest
  Cmd->>Att: attest_sign(SignContext { manifest_path, workspace, oidc, sde })
  Att->>Mn: read_manifest(manifest_path)
  Mn-->>Att: Vec ManifestEntry + schema_version + writer_profile
  Att->>Att: compute merkle_root from sorted document_id digests
  Att->>Pred: build TrainingCorpusPredicate from manifest + ctx
  Pred-->>Att: TrainingCorpusPredicate
  Att->>Stmt: InTotoStatement::new(predicateType=TRAINING_CORPUS_PREDICATE_TYPE, subject=[{name, digest:{blake3, sha256(manifest)}}], predicate)
  Stmt-->>Att: InTotoStatement
  Att->>Schema: validate(predicate against published JSON-Schema v0.3)
  Schema-->>Att: ok or Exit 8
  Att->>Att: payload_bytes = deterministic_json_vec(Statement)

  Note over Att,Oidc: --offline check happens here, before any network
  Att->>Oidc: IdentityToken::try_from(oidc_id_token JWT)
  Oidc-->>Att: typed IdentityToken
  Att->>Dse: dsse_sign(ctx, id_token, payload_type="application/vnd.in-toto+json", payload=payload_bytes)
  Dse->>Dse: payload_b64 = base64(payload_bytes)
  Dse->>Dse: PAE = "DSSEv1 " + utf8_byte_len(payload_type) + " " + payload_type + " " + utf8_byte_len(payload_b64) + " " + payload_b64
  Dse->>Ctx: SigningContext::production()
  Ctx-->>Dse: SigningContext (TUF-fetched Fulcio + Rekor + TSA trust roots)
  Dse->>Sess: SigningSession::sign_dsse(payload_type, payload_b64) via [patch.crates-io]

  Note over Sess,Fulcio: sign_dsse calls materials() internally — fork's empty-emailAddress<br/>workload-identity patch at bundle/sign.rs:97-110 stays load-bearing (V5)
  Sess->>Fulcio: request_cert_v2(x509 CSR with ephemeral pubkey + empty emailAddress, id_token)
  Fulcio-->>Sess: x509 cert chain bound to OIDC subject + issuer
  Sess->>Sess: ecdsa_p256_sign(ephemeral_key, PAE) = signature_bytes
  Sess->>Rekor: submit DSSERequestV002 { envelope: { payloadType, payload, signatures:[{sig}] }, verifiers:[{public_key, key_details}] }
  Rekor-->>Sess: tlog entry { logIndex, integratedTime, kindVersion:{kind:"dsse", version:"0.0.2"}, inclusionProof, signedEntryTimestamp }
  Sess->>Bun: assemble { mediaType=v0.3, verificationMaterial:{certificate, tlogEntries[0], timestampVerificationData}, content: DsseEnvelope { payloadType, payload, signatures:[{sig}] } }
  Bun-->>Sess: Bundle v0.3
  Sess-->>Dse: SigningArtifact (Bundle v0.3 + DsseEnvelope, NOT v0.2 + MessageSignature)
  Dse-->>Att: Bundle v0.3

  Att->>Att: bundle_json = deterministic_json_vec(bundle) — sorted-keys ProtoJSON
  Att->>Fs: write_atomic(workspace/bundles/manifest.sigstore.json, bundle_json)
  Fs-->>Att: ok
  Att->>Att: re-parse bundle JSON + extract_identity() (cert SAN + OIDC issuer)
  Att-->>Cmd: SignedAttestation { bundle_path, identity, oidc_issuer, merkle_root, predicate_type }
  Cmd-->>U: print identity + oidc_issuer + merkle_root + bundle_path + Exit 0
```

**What landed at Sprint 4 E3.5 (historical — predates the Session 1 contract flip on 2026-05-25)**:

- `attestrum sign <manifest>` clap subcommand at `crates/attestrum-cli/src/commands/sign.rs` — full message-flow as drawn, with the `PLACEHOLDER_TRAINING_CORPUS_URI` resolved to the real PROTECTED v0.3 URI string `https://attestrum.com/attestation/training-corpus/v0.3` (the v0.3 set is the initial public predicate version under the Attestrum project name; carries forward the u32 PPM field shapes from the Annex-era v0.2 schemas — see `docs/migration/v0.2-to-v0.3-attestrum-rebrand.md`).
- `SignState` + `SignEvent` pure state machine at `crates/attestrum-cli/src/lifecycle.rs` — 12 non-terminal states, 22 documented transitions, 7 exit codes (0, 1, 2, 3, 4, 5, 8 per PATH-A-BRIEF §5.2).
- Per-`sequenceDiagram` contract test at `crates/attestrum-cli/tests/sign_flow_contract.rs` (closes the §7.1 obligation).
- `sigstore` crate choice resolved at E3 (`1568721`): RustCrypto `sigstore 0.14` with `bundle/fulcio/rekor/sigstore-trust-root/rustls-tls` features, blocking signer API path. **The E3.5 implementation called `SigningSession::sign(input)` — the wrong primitive.** Session 2 replaces that call with a new `dsse_sign` module that invokes the Session-5 fork-side `SigningSession::sign_dsse(payload_type, payload)` API.
- `--source-date-epoch <SECS>` (or `SOURCE_DATE_EPOCH` env var) is REQUIRED — feeds `built_at` and `determinism.seed`. No `SystemTime::now()` reads on any predicate-build codepath per CLAUDE.md §7 (audit PR 2 `7cefc90` removed the parallel sin from the CAS layer).
- OIDC token sourcing: `--oidc-token-file <PATH>` or env var `SIGSTORE_ID_TOKEN`. Interactive OIDC + workload-identity flows share the env-var path on CI (GitHub Actions OIDC `id-token: write` writes the JWT to the env where attestrum picks it up).

**E3.5 tactical defaults** (not contract-level, surfaceable at E4 / Sprint 5):

- `ruleset_mode = Strict`, `ruleset_id = "attestrum-default"`, `ruleset_version = "v0.1.0"` are hardcoded until a real ruleset config file ships.
- `licensing_posture = Undisclosed` always; SPDX-whitelist heuristic deferred so the v0.3 bundle never embeds an under-vetted "open" claim.
- Predicate JSON-Schema validation (the `Schema` participant in the diagram) is by-construction via schemars derive on the Rust types; the `Exit 8` arrow is reserved in the lifecycle but no E3.5 codepath emits it from the sign flow (inspect's Exit 8 still applies to manifest schema-version drift).
- `SignedAttestation::identity` returned the placeholder `"sigstore-bundle-v0.3"` at E3.5; E4 enriched it via the shared cert identity-extractor at `crates/attestrum-attest/src/identity.rs`.

**Deferred (Sessions 2-5 — the X→Y hybrid)**:

- **Session 2**: build `crates/attestrum-attest/src/dsse_sign.rs` implementing the DSSE-aware sign mechanism described by this diagram. Replace the `session.sign(req.statement_payload)` call site in `crates/attestrum-attest/src/sign.rs` with `dsse_sign(ctx, id_token, "application/vnd.in-toto+json", req.statement_payload)`. Pin DSSE PAE spec test vectors as unit tests. Session 2's plan must decide the transitional path for consuming `SigningSession::sign_dsse` before Session 5 lands the fork-method (likely a `cfg`-gated mock or a temporary Attestrum-side reimplementation, founder to decide in Session 2's plan mode).
- **Session 3**: confirm `crates/attestrum-attest/src/verify.rs` stays as-is. sigstore-rs's verify side has supported DSSE bundles since 0.14.0 (`bundle/verify/models.rs` + `bundle/verify/verifier.rs` handle both `MessageSignature` and `DsseEnvelope` content variants); no verify-side rework required.
- **Session 4**: update `crates/attestrum-attest/tests/cosign_interop.rs` assertions if needed, push to CI, confirm `cosign verify-blob-attestation --new-bundle-format` exits 0 against the new Bundle v0.3.
- **Session 5**: extract `dsse_sign` to the `Attestrum/sigstore-rs` fork at `attestrum/email-optional-for-workload-identity-tokens` as a third commit; expose as `pub async fn sign_dsse(...)` on `impl SigningSession`; bump the `[patch.crates-io]` rev in the workspace `Cargo.toml`; drop the transitional Attestrum-side path (if Session 2 chose to write one).
- **Session 6 (out-of-band, async)**: upstream the fork-method as a sigstore-rs PR. Multi-week; does not block any Sprint 5 milestone.
