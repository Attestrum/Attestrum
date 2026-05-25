# Attestrum cosign-interop verify-side audit

**Purpose**: a single self-contained briefing document that multiple agents (or human reviewers) can read independently and offer a recommendation on how to proceed. The diagnosis below was produced by one Claude Code agent in a ~30-minute investigation cycle; reviewers should challenge it, not just accept it.

**Audience**: Claude Code agents, other LLM-based code reviewers, and any human collaborator the founder shares this with. Assume no prior Attestrum context; everything you need is in this file or the referenced paths.

**Date**: 2026-05-25
**Repo HEAD at audit-write**: `d3e352b` (`fix(ci): surface sigstore-rs #[source] chain at verify-side error site`)
**Remote**: `https://github.com/Attestrum/Attestrum.git` (private)
**Audit author**: Claude Opus 4.7 (1M context), invoked under the standard Attestrum CLAUDE.md harness.
**Decision sought**: which of options X / Y / Z / W (see §8) to pursue, and any objections to or refinements of the diagnosis in §6.

---

## Table of contents

1. [Document purpose and how to review](#1-document-purpose-and-how-to-review)
2. [Attestrum project context](#2-attestrum-project-context)
3. [The cosign-interop CI red — chronological history](#3-the-cosign-interop-ci-red--chronological-history)
4. [What we did today (Step 1 instrumentation)](#4-what-we-did-today-step-1-instrumentation)
5. [The captured error chain](#5-the-captured-error-chain)
6. [Today's diagnosis — root cause analysis](#6-todays-diagnosis--root-cause-analysis)
7. [Implications for the project](#7-implications-for-the-project)
8. [Fix options (X / Y / Z / W)](#8-fix-options-x--y--z--w)
9. [Constraints reviewers must respect](#9-constraints-reviewers-must-respect)
10. [Specific questions for reviewers](#10-specific-questions-for-reviewers)
11. [Pointers and references](#11-pointers-and-references)
12. [Glossary](#12-glossary)

---

## 1. Document purpose and how to review

You are being asked to do one of these things, in order of preference:

1. **Challenge the diagnosis in §6.** Is it actually correct? What might the author have missed? Are there sigstore-rs APIs or upstream behaviors the author didn't explore? Is the "signed hash ≠ verified hash" framing right?

2. **Refine the option matrix in §8.** Are options X / Y / Z / W the right options? Is there a 5th option? Is one of them mis-described?

3. **Recommend an option.** Based on the project context in §2, the constraints in §9, and your read of the diagnosis, which option should Attestrum pursue? Why? What are the second-order consequences (timeline, scope, acquirer-optionality) you weighed?

4. **Flag risks the author didn't.** Especially around CLAUDE.md §4 PROTECTED systems, PATH-A-BRIEF acquirer-optionality, or upstream sigstore-rs PR strategy.

You do **not** need to write code. This is a research / advisory pass. The agent that picks up the implementation cycle will write a new plan-mode plan based on the chosen direction.

**Read order suggested**: §2 (project) → §3 (history) → §6 (diagnosis) → §8 (options) → §10 (questions). §9 (constraints) is essential context, don't skip it. §4 / §5 / §7 / §11 / §12 are reference material.

---

## 2. Attestrum project context

### 2.1 What Attestrum is

Attestrum is a **deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles**. The pitch:

- Input: a training corpus on disk (text, image, mixed-media datasets).
- Output: a Sigstore-signed, in-toto-attested, Merkle-rooted-over-BLAKE3 "provenance bundle" that a third party can verify with **upstream cosign alone** (no Attestrum install required on the verifier side).

The headline acceptance criterion (PATH-A-BRIEF §1.5) is that **`cosign verify-blob-attestation --new-bundle-format` against the emitted bundle returns exit 0 + "Verified OK"** without any Attestrum-specific tooling.

The project pivoted in May 2026 from a "frontier-lab compliance" pitch (killed by competitive audit — frontier labs don't want their corpora auditable) to **Path A: the trust layer for open AI training data**, aimed at AI2, Pleias, EleutherAI, Black Forest Labs, Mozilla Data Collective, and Hugging Face dataset publishers.

### 2.2 Who's building it

Solo founder (Austin Munday) under Hyper Beam Media LLC, using Claude Code as the primary implementation harness. Spec-driven development workflow: detailed `.md` prompt files, plan-first gates, CHANGELOG+SESSION-LOG entries per commit, diagram-first CI gates.

90-day MVP under a six-sprint plan. Currently mid-Sprint 5.

### 2.3 Technical stack

- **Language**: Rust 1.88 (workspace pinned).
- **Crypto primitives**: BLAKE3 (corpus hashing), SHA-256 (in-toto / Sigstore interop), RFC 6962 binary Merkle tree, Sigstore Bundle v0.3 (signed-artifact format), in-toto Statement v1 (attestation payload), DSSE (signed-envelope wrapping).
- **Sigstore stack**: `sigstore-rs` 0.14.0 (Fulcio keyless cert issuance + Rekor v2 transparency log + TUF trusted-root cache). **We currently maintain a 2-commit fork** of sigstore-rs to fix a workload-identity OIDC bug — see §3 below.
- **Storage**: Content-addressed store (`.attestrum/cas/`), append-only Merkle ledger (`.attestrum/manifests/`), Parquet manifests (zstd level 3, deterministic).
- **License**: Apache-2.0 OR MIT dual-license, copyright Hyper Beam Media LLC.

### 2.4 Workspace layout (crates that matter for this audit)

```
crates/
├── attestrum-attest/      ← Sigstore Bundle v0.3 sign + verify (THE BUG IS HERE)
│   ├── src/lib.rs         ← AttestrumAttestError enum + predicate type URIs (PROTECTED)
│   ├── src/sign.rs        ← attest_sign(SignRequest) → SignedAttestation
│   ├── src/verify.rs      ← attest_verify(VerifyRequest) → VerifiedAttestation
│   ├── src/predicate.rs   ← TrainingCorpusPredicate (PROTECTED type)
│   ├── src/statement.rs   ← InTotoStatement v1 wrapper
│   └── tests/cosign_interop.rs  ← the failing integration test (#[ignore]'d; CI-only)
├── attestrum-merkle/      ← (PROTECTED) RFC 6962 Merkle over BLAKE3
├── attestrum-cas/         ← (PROTECTED) Content-addressed store
├── attestrum-ledger/      ← (PROTECTED) Append-only Merkle tile log
├── attestrum-fingerprint/ ← Text/image/ISCC fingerprinting (text normalization PROTECTED)
├── attestrum-pipeline/    ← Rayon pipeline that builds the manifest from corpus entries
├── attestrum-cli/         ← User-facing `attestrum {build,sign,verify}` subcommands
└── tools/diagram-linter/  ← Rust binary enforcing CLAUDE.md §5 diagram-first rule
```

### 2.5 CI workflows

Three workflow files under `.github/workflows/`:

- **`ci.yml`** — fmt + clippy + test (`cargo test --workspace`) + cargo-deny + diagrams. Runs on push:main + pull_request.
- **`determinism.yml`** — 4-target byte-identity matrix: `linux-x86_64-glibc`, `linux-aarch64-glibc`, `macos-aarch64-darwin`, `linux-x86_64-musl`. Asserts manifest+bundle bytes are identical across all four targets.
- **`cosign-interop.yml`** — push:main only. Installs cosign v2.5+, exchanges GHA OIDC for a sigstore-audience token, runs the `cosign_interop` test (signs a real bundle against Sigstore public-good, self-verifies, then shells out to upstream cosign and asserts round-trip).

`ci.yml` + `determinism.yml` are currently GREEN on `d3e352b`. `cosign-interop.yml` is the only red workflow.

### 2.6 Project governance documents

Three Markdown files at repo root govern the work, in priority order:

1. **`BUILD-PLAN.md`** (v0.1.1) — original technical kickoff. Sets language, primitives, workspace layout, determinism harness, signal parsers. Canonical unless `PATH-A-BRIEF.md` overrides.
2. **`PATH-A-BRIEF.md`** (v0.3.0) — Path A delta. Adds `attestrum prove` workflow, fingerprinting crate, Hugging Face publish, two new predicate types, diagram-first CI gate. Replaces Sprint 6 of BUILD-PLAN entirely.
3. **`CLAUDE.md`** — process rulebook (not technical content). Diagram-first, plan-first, session-logging, protected systems, anti-patterns.

If reviewers want to read the canonical specs themselves, those are the three files to load.

---

## 3. The cosign-interop CI red — chronological history

### 3.1 Sprint 4 E4.5 — first attempt to ship cosign-interop

Sprint 4 E4.5 (the original cosign-interop landing commit, before the project rebrand) shipped:

- The integration test at `crates/attestrum-attest/tests/cosign_interop.rs` — `#[ignore]`'d so `cargo test --workspace` lists but doesn't execute it.
- The dedicated workflow `.github/workflows/cosign-interop.yml` — push:main only; exchanges GHA OIDC for a sigstore-audience token; runs the ignored test under `--include-ignored`.

**The test was never green.** It was written under the assumption that it would work, the first CI run was deferred until the founder's first public push to `github.com/Attestrum/Attestrum`, and the first run on the GHA environment failed with the original error: `"Malformed JWT: claims JSON malformed"`.

### 3.2 Pre-Sprint-5 — JWT-parse fix (the fork)

The pre-Sprint-5 cycle (founder choice 2026-05-25) prioritized fixing cosign-interop as a "before we can build anything else" blocker. Investigation found:

- sigstore-rs 0.14.0's `Claims` struct at `src/oauth/token.rs:31` hard-required `pub email: String`.
- Workload-identity OIDC tokens (GitHub Actions, GitLab CI, GCP service accounts) **carry no `email` claim** — that's a human-OAuth concept.
- serde's "missing field `email`" was being swallowed by sigstore-rs's `.or(Err(...))?` wrapper and surfacing as the misleading `"Malformed JWT: claims JSON malformed"`.
- Bug present in 0.12.1, 0.13.0, 0.14.0, AND upstream `main` — no version bump fixes it.

A 2-commit fork at `https://github.com/Attestrum/sigstore-rs.git` was created, branch `attestrum/email-optional-for-workload-identity-tokens`:

1. **`b7ec558`** — `feat(oauth): make Claims.email optional for workload-identity tokens`. Changes `email: String` → `#[serde(default)] email: Option<String>`.
2. **`ade5422`** — `fix(bundle/sign): cast empty email str to bytes for AttributeValue::new`. Adjusts `src/bundle/sign.rs:100` to pass `email.as_deref().unwrap_or("").as_bytes()` for the CSR subject's emailAddress attribute.

Pinned in workspace `Cargo.toml`:

```toml
[patch.crates-io]
sigstore = { git = "https://github.com/Attestrum/sigstore-rs.git", rev = "ade5422143560cd795aae4ce59c593fe58090336" }
```

After this fork patch landed at commit `60af47e`, the JWT-parse failure was gone. **Sign succeeded end-to-end**: Fulcio cert issuance, Rekor v2 entry submission, bundle materialization to disk. The test then advanced to the self-verify step and failed there with the **opaque** `"signature verification failed"` error.

### 3.3 The four CLAUDE.md §7 carry-forward CI reds

A CLAUDE.md §7 TODO box was added documenting four unrelated CI reds on `main` that surfaced when the GHA environment first ran each workflow:

- **(a)** `ci.yml` audit job — `cargo-deny advisories FAILED` (transitive RUSTSEC advisories: paste-unmaintained + Marvin Attack on rsa). **CLOSED** in commit `d2f0666`.
- **(b)** `determinism.yml` — `read_only_parent_propagates_io_error` test fails on `linux-x86_64-musl` (root has CAP_DAC_OVERRIDE, bypasses chmod assertion). **CLOSED** in commit `2efcb42`.
- **(c)** `cosign-interop.yml` — sigstore-rs verify-side after the JWT-parse fix. **THIS AUDIT IS ABOUT (c).**
- **(d)** `ci.yml` diagrams — Chromium-launch fails under Ubuntu 24.04's AppArmor unprivileged-user-namespace restriction. **CLOSED** in commit `d2f0666`.

After today's `d3e352b` push, (a)/(b)/(d) are all closed. (c) is the last open red.

### 3.4 Three hypotheses (at handoff-write time)

The handoff document at `/Users/austinmunday/.claude/plans/cosign-interop-verify-side-handoff-2026-05-25.md` proposed three working hypotheses for the residual verify-side bug:

- **(i)** Empty-bytes emailAddress CSR subject produces a malformed cert that re-parse rejects on verify side. (Highest probability when the handoff was written.)
- **(ii)** Rekor v2 inclusion proof timing-sensitive against the test's 1-second sign→verify gap.
- **(iii)** sigstore-rs's verify-side has a latent bug specific to workload-identity bundles that's never been tested before.

The handoff specified that Step 1 (instrumentation, surfacing the `#[source]` chain) would produce diagnostic input to cross-reference against this table.

**Today's diagnosis says all three hypotheses are wrong shape** — see §6.

---

## 4. What we did today (Step 1 instrumentation)

Today's session executed Step 1 of the handoff cleanly. Commit `d3e352b` — `fix(ci): surface sigstore-rs #[source] chain at verify-side error site`. The change:

**File**: `crates/attestrum-attest/src/verify.rs`. Added a private helper in the `Small helpers` section:

```rust
fn format_error_chain<E: std::error::Error + ?Sized>(err: &E) -> String {
    use std::fmt::Write as _;
    let mut out = format!("[0] {err}");
    let mut current = err.source();
    let mut depth = 1usize;
    while let Some(e) = current {
        let _ = write!(out, "\n  [{depth}] {e}");
        current = e.source();
        depth += 1;
    }
    out
}
```

Applied at line 154 only (replacing `e.to_string()` with `format_error_chain(&e)` in the `verifier.verify(...).map_err(|e| AttestrumAttestError::SigstoreVerify(...))` chain). Added a unit test (`format_error_chain_walks_source_chain`) that constructs a 3-deep synthetic chain and asserts each frame appears in the rendered output.

The 5 sister `Sigstore*(e.to_string())` map_err sites (sign.rs:85/91/97/104 + verify.rs:140) were intentionally left alone per founder scope decision — they're upstream of the active cosign-interop failure (sign reaches bundle materialization; verify reaches `Verifier::production()` successfully).

**Five-gate pre-commit set passed**: fmt OK, clippy `-D warnings` OK, test 376/0/2 (was 375; +1 new unit test), diagram-linter strict 96/0, deny sources/licenses ok.

**Push triggered three workflows** on `d3e352b`. `ci.yml` and `determinism.yml` were still in progress at session end. `cosign-interop.yml` run `26408881320` completed: failure, as expected — but now with the unwrapped chain in the test failure log.

---

## 5. The captured error chain

From cosign-interop workflow run `26408881320` (commit `d3e352b`), the `cargo test cosign_interop (ignored)` step ended with:

```
test cosign_interop ... FAILED

---- cosign_interop stdout ----

thread 'cosign_interop' panicked at crates/attestrum-attest/tests/cosign_interop.rs:155:6:
attestrum_attest::verify self-verify sanity gate: SigstoreVerify("[0] signature verification failed\n  [1] Public key verification error")

failures:
    cosign_interop

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s
```

**Key observations**:

1. The chain has only **2 frames**. `[0] signature verification failed` is sigstore-rs's top-level `VerificationError` wrapper. `[1] Public key verification error` is the deepest source.

2. The chain stops at frame 1 because sigstore-rs **deliberately discards** the underlying `ring` (signature library) error via `.map_err(|_| SigstoreError::PublicKeyVerificationError)` — the underscore eats the antecedent. So even with our perfect `#[source]` walk, frame 1 is as deep as we can go without patching sigstore-rs to preserve the antecedent.

3. The panic is at `tests/cosign_interop.rs:155:6` — the `.expect("attestrum_attest::verify self-verify sanity gate")` line. The shell-out to upstream cosign at `cosign_interop.rs:162-173` **never runs** because our own self-verify gate fails first.

---

## 6. Today's diagnosis — root cause analysis

This is the part reviewers should challenge hardest. It was produced in a ~30-minute investigation cycle by tracing source through sigstore-rs's checked-out fork at `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/`.

### 6.1 Where "Public key verification error" originates

Every construction of `SigstoreError::PublicKeyVerificationError` is in `sigstore-rs/src/crypto/verification_key.rs` (lines 188-261). All 18 sites follow this pattern:

```rust
// e.g. lines 195-198 (ECDSA P-256 SHA-256 branch of verify_signature)
CosignVerificationKey::ECDSA_P256_SHA256_ASN1(pub_key) => {
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, pub_key.as_slice())
        .verify(msg, &sig)
        .map_err(|_| SigstoreError::PublicKeyVerificationError)
}
```

`UnparsedPublicKey::verify(msg, &sig)` is from the `ring` crate. It returns `Err(ring::error::Unspecified)` for any cryptographic verification failure (signature math, key parsing, algorithm mismatch, etc.). The `.map_err(|_|...)` deliberately swallows that error — there is no `#[source]` chain past this point.

The two relevant `pub` functions in this module:

- **`CosignVerificationKey::verify_signature(&self, signature, msg: &[u8])`** at line 178 — used for DSSE signature verification. `msg` is the **PAE bytes**.
- **`CosignVerificationKey::verify_prehash(&self, signature, msg: &[u8])`** at line 221 — used for MessageSignature verification. `msg` is the **raw SHA-256 digest** of the artifact.

### 6.2 Where the verify dispatch happens

`sigstore-rs/src/bundle/verify/verifier.rs:62-83`:

```rust
pub(crate) fn verify_bundle_content(
    content: &BundleContent,
    signing_key: &CosignVerificationKey,
    signature: &[u8],
    input_digest: &[u8],
) -> Result<(), SignatureErrorKind> {
    match content {
        BundleContent::MessageSignature => signing_key
            .verify_prehash(Signature::Raw(signature), input_digest)
            .map_err(SignatureErrorKind::VerificationFailed),
        BundleContent::Dsse {
            pae,
            subject_sha256_digest,
            ..
        } => {
            // For DSSE, verify the signature over the PAE bytes, not the artifact hash.
            signing_key
                .verify_signature(Signature::Raw(signature), pae)
                .map_err(SignatureErrorKind::VerificationFailed)?;
            // Also verify that the in-toto statement subject matches the artifact.
            let expected_hex = hex::encode(input_digest);
            if subject_sha256_digest != &expected_hex {
                return Err(SignatureErrorKind::Transparency);
            }
            Ok(())
        }
    }
}
```

So which arm fires for our bundle? It depends on what content shape `BundleContent` parses to. That depends on what the sign side actually emitted.

### 6.3 What sigstore-rs's `SigningSession::sign` actually produces

`sigstore-rs/src/bundle/sign.rs:144-220` defines `sign_digest` (lines 144-199) and `sign` (lines 203-220, async — the blocking wrapper at line 257 follows the same flow). The relevant lines from `sign_digest`:

```rust
// Sign artifact.
let input_hash: &[u8] = &hasher.clone().finalize();
let artifact_signature: p256::ecdsa::Signature = self.private_key.sign_digest(hasher);
let signature_bytes = artifact_signature.to_der().as_bytes().to_owned();

// ... [SCT verify, Rekor entry construction] ...

let proposed_entry = ProposedLogEntry::Hashedrekord {
    api_version: "0.0.1".to_owned(),
    spec: hashedrekord::Spec { ... },
};
// ...

Ok(SigningArtifact {
    input_digest: input_hash.to_owned(),
    cert: cert.to_der()?,
    signature: signature_bytes,
    log_entry,
})
```

And `SigningArtifact::to_bundle()` at lines 351-382:

```rust
pub fn to_bundle(self) -> Bundle {
    // ...
    let message_signature = MessageSignature {
        message_digest: Some(HashOutput {
            algorithm: HashAlgorithm::Sha2256.into(),
            digest: self.input_digest,
        }),
        signature: self.signature,
    };
    Bundle {
        media_type: Version::Bundle0_2.to_string(),
        verification_material,
        content: Some(bundle::Content::MessageSignature(message_signature)),
    }
}
```

**Key facts**:

1. `SigningSession::sign(input)` **hashes** `input` with SHA-256, **signs the hash** with the ephemeral ECDSA-P256 key, and emits a **`Hashedrekord`** Rekor entry (hash + signature pair, no envelope).
2. `SigningArtifact::to_bundle()` hard-codes `content: Some(bundle::Content::MessageSignature(...))`. The `dsseEnvelope` field is never populated.
3. `media_type` is hard-coded to `Version::Bundle0_2.to_string()` — Bundle **v0.2**, not v0.3.
4. **No `Content::DsseEnvelope` is ever constructed on the sign path.** Grepping `src/` for `Content::Dsse` / `Content::DsseEnvelope` only matches verify-side parsing of externally-produced (cosign Go) bundles.

### 6.4 What Attestrum's `attest_sign` does with this

`crates/attestrum-attest/src/sign.rs:81-155`. Relevant lines:

```rust
// 1. SigningContext::production() ...
let ctx = SigningContext::production() ... ;

// 2. Parse OIDC id_token ...
let id_token = IdentityToken::try_from(req.oidc_id_token.as_str()) ... ;

// 3. Blocking signer session ...
let session = ctx.blocking_signer(id_token) ... ;

// 4. Sign the payload bytes. Builds the DSSE envelope, signs with the
//    ephemeral private key, submits the envelope + cert chain to Rekor
//    v2, embeds the tlog entry + timestamps into the SigningArtifact.
let artifact = session
    .sign(req.statement_payload)
    .map_err(|e| AttestrumAttestError::SigstoreSign(e.to_string()))?;

// 5. Convert the SigningArtifact into a Bundle (Sigstore Bundle v0.3
//    protobuf-JSON representation).
let bundle = artifact.to_bundle();
```

The comment at line 99-101 ("Builds the DSSE envelope, signs with the ephemeral private key, submits the envelope + cert chain to Rekor v2") **describes behavior that doesn't exist**. sigstore-rs 0.14.0's `session.sign()` does none of that. It hashes the input, signs the hash, and produces a `Hashedrekord` Rekor entry.

`req.statement_payload` here is the canonical-JSON serialization of the in-toto Statement (per the test at `cosign_interop.rs:124-131`). So:

- **Signed bytes** = `SHA-256(canonical-JSON of in-toto Statement)`
- **Bundle's `messageSignature.messageDigest.hash`** = same `SHA-256(canonical-JSON of in-toto Statement)`
- **Bundle's `messageSignature.signature`** = ECDSA-P256 over that hash
- **Bundle content variant** = `MessageSignature` (NOT DSSE)
- **Bundle mediaType** = `application/vnd.dev.sigstore.bundle+json;version=0.2`

### 6.5 What Attestrum's `attest_verify` does

`crates/attestrum-attest/src/verify.rs:148-154`:

```rust
let manifest_file = File::open(req.manifest_path).map_err(AttestrumAttestError::Io)?;
verifier
    .verify(manifest_file, bundle_proto, &policy, req.offline)
    .map_err(|e| AttestrumAttestError::SigstoreVerify(format_error_chain(&e)))?;
```

In the test at `cosign_interop.rs:148-155`, `req.manifest_path` is the `manifest.parquet` file written by the `build_corpus` pipeline. So `verifier.verify(...)` receives the parquet file as its `input` argument.

Inside sigstore-rs's `Verifier::verify`, `input` is hashed via SHA-256. Then `verify_bundle_content` dispatches on the bundle's content variant. Our bundle is `MessageSignature` shape, so it calls:

```rust
signing_key.verify_prehash(Signature::Raw(signature), input_digest)
```

Where:
- `signature` = our bundle's `messageSignature.signature` bytes = ECDSA over `SHA-256(canonical-JSON of in-toto Statement)`.
- `input_digest` = `SHA-256(manifest.parquet)`.

**These don't match.** Ring rejects the signature. The error becomes `SigstoreError::PublicKeyVerificationError`. That's exactly what we're seeing.

### 6.6 The diagnosis in one sentence

Attestrum signs the SHA-256 of the **in-toto Statement JSON** but verifies the signature against the SHA-256 of the **manifest file**. These are different byte streams. The signature verification mathematically cannot pass. The cosign-interop CI red was therefore inevitable, not workload-identity-specific.

The bug pre-dates the workload-identity / OIDC work entirely. It would manifest with human-OAuth tokens too — but the test was `#[ignore]`'d from inception and the JWT-parse failure shadowed it until commit `60af47e` peeled the JWT-parse layer off.

### 6.7 Confidence level

**Author's confidence: ~85%.** Reasons for the 15% doubt:

- The author did not run the test locally with a working OIDC token and inspect the actual produced bundle JSON. There might be a code path in sigstore-rs the author missed (e.g., a feature flag that toggles DSSE bundle emission). But: the author grep'd `Content::Dsse`, `Content::DsseEnvelope`, `Dsse`, `in_toto`, `InToto` across the full sigstore-rs source tree and found zero construction sites on the sign path.
- The author did not check sigstore-rs's `cosign/` module (`src/cosign/intoto.rs`, `src/cosign/signature_layers.rs`) deeply. Maybe there's a higher-level cosign-emulation API that does produce DSSE bundles. But: that module is `default-features = false`-disabled per workspace `Cargo.toml:89`, and Attestrum doesn't import it.
- The author did not exhaustively check whether sigstore-rs upstream `main` or any unreleased 0.15.x has a DSSE sign API. The author's evidence is the 0.14.0-equivalent fork rev `ade5422`; behavior in newer revs is not directly verified.

Reviewers should specifically attack:
- The "no DSSE sign API in sigstore-rs 0.14.0" claim. Counter-evidence kills the diagnosis.
- The "signed-hash ≠ verified-hash" framing. Is there a sigstore-rs feature flag or builder option the author missed that changes this?
- The author's assumption that `req.statement_payload` ≠ `req.manifest_path` content. Could they be the same in some test configuration? (Answer: no — `statement_payload` is canonical-JSON of an in-toto Statement, `manifest_path` is a binary parquet file. Different formats.)

---

## 7. Implications for the project

If the diagnosis in §6 is correct, the consequences are substantial:

### 7.1 The "Sigstore Bundle v0.3 with in-toto Statement" promise is currently false

Attestrum has been advertising (in BUILD-PLAN, PATH-A-BRIEF, predicate Rust types, dataset card README rendering, etc.) that it emits a **Sigstore Bundle v0.3 with the training-corpus predicate embedded as the DSSE envelope payload**. What it actually emits is a **Bundle v0.2 MessageSignature** signing the in-toto Statement's hash.

The bundles to date are cryptographically well-formed and could be verified — but only by a verifier that hashes the **same in-toto Statement JSON** the signer hashed. Cosign Go's `verify-blob-attestation --new-bundle-format` expects DSSE envelope semantics, so **cosign cannot verify our bundles** today regardless of which OIDC token type is used.

### 7.2 PATH-A-BRIEF §1.5 acceptance criterion is broken at the architectural level

The headline promise (§1.5) is **"a third party with zero Attestrum installed can verify our signed bundles using only the upstream `cosign verify-blob-attestation --new-bundle-format` binary"**. With MessageSignature-Bundle-v0.2 emission, cosign cannot do this. The cosign-interop CI workflow was the gate that caught this.

### 7.3 Two PROTECTED surfaces are entangled but not directly broken

CLAUDE.md §4 lists two PROTECTED surfaces in `attestrum-attest`:

- **Predicate type URIs** (`training-corpus/v0.3`, `inclusion-proof/v0.3`, `non-inclusion-proof/v0.3`). These are immutable. The bundles emit them as the `predicateType` field of the in-toto Statement, and that field IS correctly emitted (it's inside the canonical-JSON bytes we hash + sign — just not wrapped in a DSSE envelope as documented).
- **CAS directory layout** — not relevant to this bug.

Fix options that don't change the predicate URIs are PROTECTED-safe. Fix options that change the bundle shape (DSSE wrapping) don't touch the predicate URIs either — only how those URIs travel inside the bundle.

### 7.4 Sprint 5 work depends on these bundles being correct

Sprint 5 is mid-execution. Active threads:

- **S5-D1 E5** (next E-commit, currently being planned) — API freeze + cross-target byte-determinism gate. The byte-determinism gate compares emitted bundles across the 4 determinism targets. If we change the bundle shape (DSSE wrapping), all golden bytes change.
- **Sprint 5 E11** — cosign-interop tests for proof predicates (inclusion-proof, non-inclusion-proof). Depends on the training-corpus cosign-interop being green first.
- **`attestrum prove` workflow** — emits inclusion-proof / non-inclusion-proof predicates. If the training-corpus bundle shape changes, the proof predicates' bundle shape probably also changes.

A bundle-shape change has reach beyond just `crates/attestrum-attest/`.

### 7.5 The fork's empty-emailAddress patch is orthogonal

The fork patches we landed at commit `60af47e` fix a real bug (workload-identity JWT parsing). That fix should stay regardless of how the cosign-interop bug is resolved. None of the four fix options below require touching the fork.

### 7.6 Acquirer-optionality is unaffected by the bug itself, but very much affected by the fix choice

PATH-A-BRIEF §12 ("Acquirer-optionality hygiene") says we want Attestrum to be **substrate, not a branded silo** — an acquirer should be able to drop Attestrum and the OSS users still have cosign-verifiable bundles. The current MessageSignature shape is acquirer-hostile (cosign doesn't verify it). Option X / Y restore the optionality. Option Z accepts permanent cosign-incompatibility, which is acquirer-hostile.

---

## 8. Fix options (X / Y / Z / W)

Four options, ordered by scope from largest to smallest.

### Option X — Build the DSSE envelope ourselves inside `attest_sign`

**Mechanic**: Replace `session.sign(req.statement_payload)` with manual DSSE construction:

1. Build the DSSE PAE bytes: `"DSSEv1 " + len(payloadType) + " " + payloadType + " " + len(payload) + " " + payload`. payloadType = `application/vnd.in-toto+json`.
2. Use sigstore-rs's lower-level signing primitives (Fulcio CSR issuance + ECDSA-P256 signing) to obtain a Fulcio cert + sign the PAE bytes (not the input hash).
3. Construct a Bundle v0.3 protobuf-JSON object with `Content::DsseEnvelope`, populating `dsseEnvelope.payloadType`, `dsseEnvelope.payload` (base64-encoded), and `dsseEnvelope.signatures[0].sig`.
4. Submit a DSSE-flavored Rekor entry (`Dsse` or `Intoto` Rekor v2 entry type, not `Hashedrekord`).
5. Assemble VerificationMaterial with cert chain + Rekor proof + integratedTime + logIndex.

**Effort estimate**: 1-3 days of focused work. ~150-300 lines of code in `attest_sign`. Comparable amount of test code. Possibly a fork-side patch to sigstore-rs to expose the lower-level signing primitives if they're `pub(crate)` today.

**Pros**:
- Restores PATH-A-BRIEF §1.5 promise (cosign-interop becomes achievable).
- All Rust, in-process — fits the founder's "single deterministic Rust CLI" thesis.
- Preserves the existing `attest_sign` / `attest_verify` API shape — callers don't change.
- The fork's empty-emailAddress patch can stay or be dropped (orthogonal).

**Cons**:
- Substantial new code in a sensitive subsystem.
- Re-implementing DSSE PAE construction has subtle byte-determinism risks (UTF-8 length vs byte length, trailing space handling, etc.). Mistakes here would silently emit cosign-incompatible bundles even AFTER the "fix".
- Rekor v2 has distinct entry types for hashed-blob signatures vs DSSE attestations; submitting to the wrong one produces a valid bundle that cosign rejects with a different error.
- Doesn't help upstream sigstore-rs users in the same situation.

**Risk if it goes wrong**: bundles emitted between the "fix" landing and Step 5 of the next plan being verified might be cosign-incompatible in subtler ways. The cosign-interop CI workflow is the gate that catches this.

### Option Y — Upstream a `sign_dsse` API to sigstore-rs

**Mechanic**: Add a new method on `SigningSession` to sigstore-rs that takes `(payload_type: &str, payload: &[u8])` and produces a DSSE-envelope Bundle v0.3 + a DSSE-flavored Rekor entry. Either land it as a third commit on our existing `attestrum/email-optional-for-workload-identity-tokens` fork branch (or a separate branch) and use it from Attestrum, OR submit it upstream as a PR and wait for it to merge into a 0.15.x release.

**Effort estimate**:
- Upstream-PR-then-wait path: multi-week (upstream review + release cadence). Blocks cosign-interop indefinitely.
- Fork-third-commit path: similar code complexity to Option X (~150-300 lines) but landing inside sigstore-rs's source tree rather than Attestrum's `attest_sign`. ~2-4 days end-to-end including the round-trip through cargo + workspace `[patch.crates-io]` rev bump.

**Pros**:
- Cleanest separation of concerns: Attestrum's `attest_sign` stays a thin wrapper around sigstore-rs.
- Helps other sigstore-rs users in the same situation. Good upstream citizenship.
- If we land the PR upstream and it merges, the fork can be dropped entirely (along with the existing 2-commit JWT-parse patch).
- Strong Path A narrative: "Attestrum contributed a missing piece to the open Sigstore Rust SDK".

**Cons**:
- Slower than Option X if upstream PR review drags.
- The fork's 3-commit shape is the same complexity as Option X — we trade location of the code for ecosystem visibility.
- The DSSE-PAE-determinism risk in Option X (Cons #2) is the same here.

**Risk if it goes wrong**: same byte-determinism risks as Option X. Plus: if upstream rejects the PR design, we eat the fork-maintenance burden indefinitely.

### Option Z — Accept MessageSignature semantics; change verify to match

**Mechanic**: Change `attest_verify` so that the byte stream it passes to `verifier.verify(input, ...)` is the **canonical in-toto Statement JSON** (the same bytes the signer signed), not the manifest file. Verify the bundle's signature against the statement's hash. Then *separately* verify the in-toto Statement's `subject[0].digest.sha256` field matches `SHA-256(manifest.parquet)`.

The bundle attests to the in-toto Statement; the Statement attests to the manifest. Two-step verification: (1) crypto on Statement, (2) Statement.subject digest match against manifest.

**Effort estimate**: ~1 day. Small `attest_verify` code change (re-derive the statement bytes from the bundle's MessageSignature digest indirection, OR store the statement bytes alongside the bundle and load both). New tests covering the two-step verification.

**Pros**:
- Smallest change. No fork patches, no sigstore-rs upstream PR.
- Conceptually clean: the in-toto Statement is what's actually signed, the manifest digest is what the Statement attests to.
- The fork's 2-commit JWT-parse patch can stay unchanged.

**Cons**:
- **PATH-A-BRIEF §1.5 acceptance criterion stays broken.** Cosign Go cannot verify our bundles ever, under this option.
- **Acquirer-hostile.** PATH-A-BRIEF §12 explicitly calls out "any acquirer could run this without breaking the OSS users". A non-cosign-verifiable bundle violates this.
- We have to publish + document our own verifier (the static `verify.html` Sprint 6 deliverable becomes more important; current README has to disclaim cosign-incompatibility).
- The Bundle v0.2 + MessageSignature shape is non-standard for "in-toto attestation" use — sigstore-go, sigstore-python, sigstore-java all assume DSSE for in-toto. We become an outlier.

**Risk if it goes wrong**: minor. If we ship Z and later want to retrofit X / Y, the bundle format changes — every bundle issued under Z would need re-issuance under X / Y. But this is also true for X→Y or Z→X transitions.

### Option W — Out-of-process (shell out to cosign Go for signing)

**Mechanic**: Replace `session.sign(req.statement_payload)` with a subprocess call to the cosign binary's `sign-blob --attestation` (or similar) command. Cosign Go natively builds DSSE Bundle v0.3 with DSSE Rekor entries. Attestrum keeps the in-process verify path (sigstore-rs `Verifier::verify` already handles DSSE bundles correctly).

**Effort estimate**: ~1-2 days. New subprocess-orchestration code in `attest_sign`. `which cosign` precondition check. OIDC token pass-through (cosign reads `SIGSTORE_ID_TOKEN` env var or `--identity-token` flag).

**Pros**:
- Cosign Go is the canonical Sigstore client. Behavior is by construction correct.
- Doesn't require us to maintain a DSSE construction code path.
- Fast to implement, cheap to verify (the cosign-interop CI workflow tests the same code path Attestrum uses).

**Cons**:
- Big architectural shift. Abandons the "single deterministic Rust CLI" thesis.
- Cosign Go is a non-trivial runtime dependency. Adds an external binary requirement to the project's deployment story.
- Cross-platform: cosign Go binaries exist for Linux/macOS/Windows, but adds friction for users on non-mainstream platforms.
- Determinism story is harder — cosign Go's bundle output may not be bit-for-bit identical across runs (depends on timestamps, ECDSA k-nonces if not RFC 6979).
- The fork's 2-commit JWT-parse patch becomes dead code (cosign Go handles workload-identity OIDC natively). We'd drop the patch + fork.
- Acquirer story: cosign Go is its own Linux Foundation project. Attestrum being a "cosign Go wrapper" is a smaller acquisition target than "Sigstore Rust SDK consumer that contributed upstream".

**Risk if it goes wrong**: subprocess invocation is a known anti-pattern in BUILD-PLAN §6.2 (which assumed in-process Rust). Reverting this is high-friction.

### Option matrix summary

| Option | Effort | Restores cosign-interop | Architectural impact | Best for |
|--------|--------|------------------------|----------------------|----------|
| **X** — Build DSSE in `attest_sign` | 1-3 days | Yes | Medium (new code in `attestrum-attest`) | Pragmatic, ship-fast |
| **Y** — Upstream sigstore-rs sign_dsse | 2-4 days (fork) or weeks (upstream PR) | Yes | Low (sigstore-rs is the right place for this) | Long-term ecosystem fit + acquirer narrative |
| **Z** — Accept MessageSignature, change verify | ~1 day | NO | Smallest | Time-pressured ship, abandon cosign promise |
| **W** — Shell out to cosign Go | 1-2 days | Yes | Largest (abandons in-process Rust) | If founder no longer wants Rust-only thesis |

**Audit author's recommendation**: **Option X**, with the understanding that the work upstreams cleanly to sigstore-rs as a follow-up (effectively becoming Option Y in retrospect once the upstream PR merges). Option X delivers a working cosign-interop in the shortest timeline that preserves the PATH-A-BRIEF promises and the founder's Rust-only thesis. Option Y is more virtuous but the upstream-PR latency is a real risk to Sprint 5 timelines.

Reviewers: challenge this. The author is not strongly attached to X over Y. Z is acquirer-hostile so the author argues against it unless the founder has decided to defer the cosign-interop promise to v0.2.

---

## 9. Constraints reviewers must respect

These are non-negotiable absent explicit founder approval. From CLAUDE.md + PATH-A-BRIEF + BUILD-PLAN:

### 9.1 Protected systems (CLAUDE.md §4)

Touching any of these requires explicit founder approval in the commit message footer:

- `crates/attestrum-merkle/` — RFC 6962 binary Merkle over BLAKE3.
- `crates/attestrum-attest/` **predicate type URIs** — `attestrum.com/attestation/{training-corpus,inclusion-proof,non-inclusion-proof}/v0.3`. URI changes require v0.4 bump + migration doc + in-toto vetted-catalog re-submission.
- `crates/attestrum-cas/` directory layout — `.attestrum/objects/`, `.attestrum/cas/`, `.attestrum/manifests/`.
- `crates/attestrum-ledger/` tile layout — append-only.
- `tests/golden/article53/` — EU Article 53 template goldens.
- `crates/attestrum-fingerprint/` text normalization — NFC + lowercase + whitespace collapse is locked.

**Bundle format choice (X/Y/Z/W) does NOT change predicate URIs.** The URI travels inside the in-toto Statement's `predicateType` field regardless of how the Statement is wrapped (DSSE envelope vs MessageSignature payload). Safe.

### 9.2 Dependency discipline (CLAUDE.md §8)

- No new crate without explicit founder approval. Surface name + version + license + reason.
- No GPL / AGPL deps. Apache-2.0 / MIT / BSD / MPL-2.0 / Unlicense / CC0 only. Plus the approved transitive-only exceptions in `deny.toml` (NCSA, ISC, MIT-0, Zlib, CDLA-Permissive-2.0, BSL-1.0, Unicode-3.0).
- No `unsafe` outside vetted FFI shims.
- No git-pinned deps except the two already approved (`hf-hub` per PATH-A-BRIEF §2.3, `sigstore-rs` Attestrum fork per Sprint 5 Q1).
- No alpha / pre-release versions. Minor versions pinned in workspace `Cargo.toml`.

**Implications**: Option X's "use sigstore-rs's lower-level signing primitives" might require exposing currently-`pub(crate)` items via a fork patch — that's fine (the existing fork branch can absorb it). But adding a *new* DSSE library (e.g., `in-toto-rs`, `dsse-rs` if such crates exist) requires founder approval.

### 9.3 Diagram-first (CLAUDE.md §2 + §5)

Any new module, public API, error path, or multi-party flow needs a Mermaid diagram under `docs/diagrams/<sprint-or-area>/` BEFORE production code. Frontmatter mandatory (`title`, `models`, `source_of_truth`, `last_verified`, `diagram_type`).

**Implications**: All four options change the sign flow. The relevant diagram is `docs/diagrams/sprint-4/sign-flow.md`. Reviewers should expect any execution-cycle plan to start with a sign-flow.md diagram update.

### 9.4 Plan-first (CLAUDE.md §3)

Every feature / fix / refactor starts in plan mode. Plan + approval gate is at the founder. No code until explicit "go".

**Implications**: regardless of which option is chosen, the next agent starts in plan mode and surfaces a per-commit breakdown for founder approval.

### 9.5 Five pre-commit gates (CLAUDE.md §7)

`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `cargo run -p diagram-linter -- check --strict` + `cargo deny check sources licenses`. All must pass locally before push.

### 9.6 Push cadence (CLAUDE.md §6.1)

Every local commit pushes to `origin/main` immediately. No batching except in deliberate multi-commit landing sequences.

### 9.7 Session records (CLAUDE.md §6)

Every commit appends a session entry to both `CHANGELOG.md` and `SESSION-LOG.md`. Same content, append-only.

### 9.8 Acquirer-optionality (PATH-A-BRIEF §12)

- Public type URIs only (Sigstore Bundle, in-toto Statement, Croissant, CycloneDX ML-BOM all use canonical public URIs).
- No vendor lock-in — every artifact verifiable by upstream tools.
- Domain ownership migratable (in-toto's New Predicate Guidelines support URI renames).
- Hub-publish is one of several targets.

**Implications**: Option Z is acquirer-hostile (breaks the "verifiable by upstream cosign" promise). Options X / Y / W preserve acquirer-optionality.

### 9.9 What this audit is NOT asking reviewers to do

- Write code. (Implementation cycle is a future plan-mode session.)
- Decide on PROTECTED-surface changes. (None of X/Y/Z/W require them.)
- Re-litigate the fork-vs-shell-out trade-off for the JWT-parse fix at commit `60af47e`. (Already decided; fork stays.)
- Propose adding the cosign Go binary as a build-time dep. (Option W *does* propose this; if reviewers favor W, the cosign-Go-binary-dependency is the explicit trade-off.)
- Propose changing the predicate URIs. (PROTECTED — out of scope.)

---

## 10. Specific questions for reviewers

Concrete, decision-relevant. Reviewers don't need to answer every one — answer the ones where you have a strong view.

### 10.1 Diagnosis verification

1. **Is the §6 diagnosis correct?** Specifically: is the author right that sigstore-rs 0.14.0 has no DSSE sign API on the public path, and that `SigningSession::sign(input)` produces a MessageSignature Bundle v0.2? If you find evidence to the contrary (a feature flag, a missed API, an unreleased 0.15.x DSSE method), the option matrix changes.

2. **Is the "signed hash ≠ verified hash" framing the right framing?** Alternative framings to consider:
   - "sigstore-rs's `Verifier::verify` doesn't know what type of artifact it's verifying — the caller has to feed it the right bytes."
   - "Attestrum's API contract is ambiguous — `req.manifest_path` could mean 'verify the bundle against the manifest' or 'verify the bundle's structural integrity, the manifest is just for digest cross-checking'."
   - Some other framing the author missed.

3. **Did the author miss anything in sigstore-rs's `cosign/` module** (the OCI-registry features that are disabled in our `default-features = false` config)? Specifically `src/cosign/intoto.rs` and `src/cosign/signature_layers.rs`. The author's grep showed these files mention DSSE; the author did not deep-read them.

### 10.2 Option ranking

4. **Which of X / Y / Z / W do you recommend?** Defend the choice in terms of: timeline, scope, acquirer-optionality (PATH-A-BRIEF §12), and Sprint 5 dependencies (§7.4 above).

5. **Is the author's "Option X with eventual Y upstreaming" recommendation right?** Or is the right move to invest in Y from day one (taking the longer timeline as the cost of cleaner architecture)?

6. **Is there a 5th option the author missed?** Examples to consider but probably reject:
   - Use a different Rust Sigstore SDK (none exist mature enough).
   - Build our own Sigstore client from scratch (massive scope, no upside over Y).
   - Pivot the bundle format to something non-Sigstore (kills PATH-A-BRIEF §1.5).
   - Defer cosign-interop indefinitely (PATH-A-BRIEF §1.5 acceptance criterion violation).

### 10.3 Timing and Sprint 5 implications

7. **When should this work land?** Options:
   - **Now**, as a pre-S5-D1-E5 blocker (Sprint 5's API freeze should know the final bundle shape).
   - **After S5-D1 E5**, accepting that E5 freezes a wrong bundle shape and we re-freeze later.
   - **At Sprint 6**, deferring to "after Sprint 5 ships". This is the worst option for acquirer-optionality but the lowest disruption.

8. **Should the existing fork's `60af47e` JWT-parse patch survive the fix?** Yes for all four options — the workload-identity JWT bug is real and unrelated. But reviewers should sanity-check this.

### 10.4 Risks the author may have missed

9. **Are there Sprint 5 dependencies the author didn't enumerate in §7.4?** Specifically, look at:
   - `attestrum-pipeline`'s manifest construction (does the manifest format depend on bundle shape?).
   - `attestrum-fingerprint`'s `FingerprintBundle` structure (does it reference the Sigstore bundle?).
   - The `verify.html` Sprint 6 deliverable (does it parse Bundle v0.2 or v0.3?).
   - The dataset card README rendering (does it embed bundle JSON?).

10. **Is the Article 53 golden test (`tests/golden/article53/`) affected?** Author thinks no, but reviewers with EU AI Act compliance context should sanity-check.

11. **Determinism**: does any of the four options have hidden determinism risks? Cross-target byte-identity is a load-bearing CLAUDE.md §7 invariant. ECDSA k-nonces, DSSE PAE serialization (UTF-8 vs byte length), and Rekor entry serialization are the obvious risk surfaces.

### 10.5 Communication

12. **Should the founder be looped in BEFORE the next plan-mode planning cycle starts, or only AFTER?** The handoff doc said "hypothesis (iii) confirms → surface to founder" — but the diagnosis here is bigger than (iii). The audit author is treating this audit-doc-creation step as the surface-to-founder gate; the next planning cycle proceeds from the option the founder picks.

---

## 11. Pointers and references

### 11.1 Files in the Attestrum repo

| File | What's relevant |
|------|------|
| `crates/attestrum-attest/src/sign.rs` | The `attest_sign` function with the wrong assumption in its comments + the call to `session.sign(req.statement_payload)` |
| `crates/attestrum-attest/src/verify.rs` | The `attest_verify` function and the `verifier.verify(manifest_file, ...)` site at line 152-154 |
| `crates/attestrum-attest/src/lib.rs` | `AttestrumAttestError` enum + PROTECTED predicate type URIs |
| `crates/attestrum-attest/tests/cosign_interop.rs` | The failing integration test |
| `crates/attestrum-attest/src/statement.rs` | `InTotoStatement` v1 wrapper |
| `crates/attestrum-attest/src/predicate.rs` | `TrainingCorpusPredicate` (PROTECTED v0.3 schema) |
| `.github/workflows/cosign-interop.yml` | The workflow that runs the test under `--include-ignored` |
| `docs/diagrams/sprint-4/sign-flow.md` | Mermaid sequenceDiagram for the sign flow (will need updating in the next plan cycle) |
| `docs/diagrams/sprint-4/verify-flow.md` | Mermaid sequenceDiagram for the verify flow |
| `Cargo.toml` (workspace) | The `[patch.crates-io]` entry pinning sigstore-rs to the Attestrum fork |
| `BUILD-PLAN.md` §3.4, §6.2 | Sigstore Bundle v0.3 + in-toto v1 contracts |
| `PATH-A-BRIEF.md` §1.5, §12, Part 2, Part 6 Sprint 4 | Cosign-interop acceptance criterion + acquirer-optionality + dependency table + Sprint 4 deliverables |
| `CLAUDE.md` §3, §4, §6, §7, §8, §14 | Plan-first + PROTECTED + session-records + five-gate + dependency-discipline + anti-patterns |
| `CHANGELOG.md` (top entries) | Per-commit history including today's `d3e352b` |
| `SESSION-LOG.md` (top entries) | Same as CHANGELOG with extra "Findings" context |

### 11.2 Plan documents in `~/.claude/plans/`

| File | Purpose |
|------|---------|
| `cosign-interop-verify-side-handoff-2026-05-25.md` | The handoff doc the agent picked up at session start; contains the original 3-hypothesis table |
| `you-re-picking-up-stateless-marshmallow.md` | Today's Step 1 execution plan + status log |

### 11.3 sigstore-rs source (Attestrum fork, rev `ade5422`)

Cached at `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/`. Key files:

| File | What's relevant |
|------|------|
| `src/bundle/sign.rs:144-220` | `SigningSession::sign_digest` + `sign` — produces MessageSignature, not DSSE |
| `src/bundle/sign.rs:347-383` | `SigningArtifact::to_bundle()` — hard-codes `Content::MessageSignature` + `Version::Bundle0_2` |
| `src/bundle/sign.rs:97-110` | The fork's empty-emailAddress patch in the CSR-subject construction |
| `src/bundle/verify/verifier.rs:47-85` | `verify_bundle_content` — dispatches on `BundleContent::{MessageSignature,Dsse}` |
| `src/bundle/verify/models.rs:39-55` | `compute_pae(payload_type, payload)` — DSSE PAE encoding |
| `src/crypto/verification_key.rs:178-216` | `verify_signature` — the function emitting `PublicKeyVerificationError` for DSSE bundles |
| `src/crypto/verification_key.rs:221-267` | `verify_prehash` — same error variant for MessageSignature bundles |
| `src/errors.rs:88-89` | `SigstoreError::PublicKeyVerificationError` variant definition (no `#[source]` — leaf error) |
| `src/oauth/token.rs` | The fork's `email: Option<String>` patch |
| `src/bundle/intoto.rs` | In-toto Statement v1 parsing (verify-side only; no sign API) |
| `src/cosign/intoto.rs` | (Disabled in our config — `default-features = false`) Possibly relevant DSSE/in-toto APIs the author didn't deep-read |

### 11.4 External specs and references

| Resource | Why it matters |
|----------|----------------|
| Sigstore Bundle v0.3 spec | https://docs.sigstore.dev/about/bundle/ — canonical Bundle format spec |
| in-toto Statement v1 spec | https://github.com/in-toto/attestation/blob/main/spec/v1/statement.md |
| DSSE spec | https://github.com/secure-systems-lab/dsse |
| Cosign Go reference (sign_blob.go) | https://github.com/sigstore/cosign — search `sign_blob.go` for `subject` + `emailAddress` (cosign Go's CSR-subject behavior) |
| sigstore-rs GitHub | https://github.com/sigstore/sigstore-rs |
| Attestrum/sigstore-rs fork | https://github.com/Attestrum/sigstore-rs (Hyper Beam Media LLC-owned) |
| Attestrum repo | https://github.com/Attestrum/Attestrum (Hyper Beam Media LLC-owned) |
| `cosign verify-blob-attestation` docs | `cosign verify-blob-attestation --help` after installing cosign v2.5+ |

### 11.5 Recent CI runs

| Workflow | Latest run | Commit | Conclusion |
|----------|-----------|--------|------------|
| `ci.yml` | `26408881266` | `d3e352b` | in_progress at session end |
| `determinism.yml` | `26408881268` | `d3e352b` | in_progress at session end |
| `cosign-interop.yml` | `26408881320` | `d3e352b` | failure (expected — chain captured) |
| `ci.yml` | `26406850403` | `2efcb42` | success |
| `determinism.yml` | `26406850306` | `2efcb42` | success |
| `cosign-interop.yml` | `26406850307` | `2efcb42` | failure (pre-Step-1; the OLD opaque error) |

Use `gh run view <run-id> -R Attestrum/Attestrum --log-failed` to fetch failed-step logs.

---

## 12. Glossary

- **Attestation**: in this context, a signed JSON document (in-toto Statement) describing a software artifact's provenance.
- **Bundle v0.2 / v0.3**: Sigstore's canonical signed-artifact format versions. v0.3 added support for DSSE-envelope-wrapped attestation bundles; v0.2 only supported MessageSignature (raw blob signatures).
- **CSR**: Certificate Signing Request (X.509). The blob we send to Fulcio asking for a cert. Includes the requestor's public key + a subject DN + a signature over the request contents.
- **DSSE**: Dead Simple Signing Envelope (in-toto). Wraps a payload + payloadType in a canonical envelope; signatures are over the envelope's PAE bytes, not the raw payload. Format: `{"payloadType": "...", "payload": base64(...), "signatures": [{"sig": base64(...)}]}`.
- **Fulcio**: the Sigstore CA. Accepts an OIDC token + a CSR; issues a short-lived (~10 minute) cert binding the OIDC identity to a public key.
- **in-toto**: a software-supply-chain attestation framework. Defines the Statement v1 format (subject, predicateType, predicate).
- **MessageSignature**: the simpler Sigstore bundle content variant. Signs `SHA-256(input)` directly. No envelope. Used by cosign's `sign-blob` for raw-blob signing.
- **OIDC**: OpenID Connect. The identity protocol Sigstore uses for "who's signing". Workload-identity OIDC (GHA, GitLab, GCP) carries no `email` claim — that's a human-OAuth concept.
- **PAE**: Pre-Authentication Encoding. The canonical byte string DSSE signs over: `"DSSEv1 " + len(payloadType) + " " + payloadType + " " + len(payload) + " " + payload`.
- **Predicate**: in-toto's term for the typed JSON payload inside a Statement. Attestrum's predicate type URIs are PROTECTED.
- **Rekor**: the Sigstore transparency log. Append-only Merkle log of signed entries. v2 has multiple entry types — `Hashedrekord` (used by MessageSignature bundles), `Dsse` / `Intoto` (used by DSSE bundles).
- **SAN**: Subject Alternative Name (X.509 cert extension). Carries the OIDC identity (e.g., GHA workflow URL, email address, etc.).
- **SCT**: Signed Certificate Timestamp. Cryptographic proof that Fulcio logged the cert issuance to a CT log.
- **Sigstore**: keyless code signing infrastructure. Issues ephemeral certs from a CA (Fulcio) signed under a transparency log entry (Rekor).
- **Statement (in-toto v1)**: the attestation JSON: `{"_type": "https://in-toto.io/Statement/v1", "subject": [{"name": "...", "digest": {...}}], "predicateType": "...", "predicate": {...}}`.
- **TUF**: The Update Framework. The protocol Sigstore uses to distribute its trusted-root keys + revocation state.
- **Workload-identity OIDC**: OIDC issued by a machine, not a human (GHA OIDC, GitLab CI OIDC, GCP service-account-token OIDC). No `email` claim. Identity is encoded in `sub` / `iss` claims.

---

*Audit document drafted 2026-05-25 by Claude Opus 4.7 (1M context) at HEAD `d3e352b`. Reviewers: please log your reading + recommendation as a separate `## [YYYY-MM-DD] <agent-name>` section appended to this file, or as a separate file alongside it. The founder will read and decide. Comments on §6 (the diagnosis) are especially welcome.*
