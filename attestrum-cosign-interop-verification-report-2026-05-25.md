# V1-V7 verification report

**Date**: 2026-05-25
**Agent**: Claude Opus 4.7 (1M context) — execution of `/Users/austinmunday/.claude/plans/you-re-picking-up-reactive-tiger.md`
**Parent commit**: `f6eff89` (`test(cosign-interop): V1 diagnostic instrumentation — capture bundle mediaType + content variant from CI`)
**Parent at handoff-open**: `3f4df46` (audit pack land)
**Reference**: §5 of `attestrum-cosign-interop-decision-2026-05-25.md` (the V1-V7 verification cycle); fills in the §10 verification-log table.

---

## 1. Headline

**Diagnosis confirmed at ~99% confidence.** Direct CI evidence (V1) shows the bundle is `application/vnd.dev.sigstore.bundle+json;version=0.2` shaped with a `messageSignature` content variant — no `dsseEnvelope` anywhere. This matches the audit pack's structural-byte-mismatch diagnosis exactly: sigstore-rs 0.14.0's `SigningSession::sign` produces a Bundle v0.2 MessageSignature signing `SHA-256(canonical in-toto Statement JSON)`, while `attest_verify` asks sigstore-rs to verify against `SHA-256(manifest.parquet)`. Signed bytes ≠ verified bytes → ring rejects → `PublicKeyVerificationError`. The audit-pack's hypothesis that this is structural (not workload-identity-specific) is now load-bearing.

V2-V7 land as follows: **V2 corrects Reviewer 1's hedge** — cosign Go (via sigstore-go) submits Rekor v2 `DSSERequestV002` entries of kind `dsse`, not `intoto`. **V3 closes audit §10.1.3** — the disabled sigstore-rs `cosign/` module is OCI-layer / re-export only, no hidden DSSE-sign API exists anywhere in sigstore-rs 0.14.0 at fork rev `ade5422`. **V4 enumerates 4 `pub(crate)` items + 5 private fields** the X→Y hybrid's eventual fork patch will need to expose. **V5 confirms** the fork's empty-emailAddress patch site stays load-bearing under Option X's code path. **V6 finds zero matches** for sigstore bundle bytes in `attestrum-fingerprint` — no cascade risk. **V7 reveals a structural mismatch** between sigstore-rs's design (client-side x509 CSR with emailAddress attribute) and cosign Go's design (Fulcio v2 JSON proof-of-possession, no x509 CSR at all) — the K4 omit-vs-empty-bytes question is moot under cosign Go comparison.

**Net effect**: all seven checks green. Plan-mode Session 1 (sign-flow.md diagram update) ready to open. Three concrete inputs for Session 1's diagram: (a) Rekor entry kind = `dsse` (`DSSERequestV002`), (b) target mediaType = `application/vnd.dev.sigstore.bundle+json;version=0.3`, (c) target content variant = `dsseEnvelope` with `payloadType = application/vnd.in-toto+json`.

---

## 2. Checklist

| Check | Status | Result | Notes |
|-------|--------|--------|-------|
| V1 — bundle content variant + mediaType | **pass** | `Bundle v0.2` + `messageSignature` (no `dsseEnvelope`) | CI run [`26412678308`](https://github.com/Attestrum/Attestrum/actions/runs/26412678308) at commit `f6eff89`, captured at `2026-05-25T17:40:57Z` |
| V2 — cosign Go Rekor entry type for attestations | **pass** | `dsse` (`DSSERequestV002`) — NOT `Intoto` | sigstore-go `pkg/sign/transparency.go` — Reviewer-1 hedge correction |
| V3 — sigstore-rs `cosign/intoto.rs` + `signature_layers.rs` deep-read | **pass** | No hidden DSSE-sign API | `cosign/intoto.rs` is a 20-line re-export shim; `signature_layers.rs` DSSE refs are verify-side only |
| V4 — sigstore-rs lower-level primitives for Option X | **pass** | 1 fn + 3 SigningSession fields + 4 SigningArtifact fields need fork-side `pub` exposure | Plus `compute_pae` — re-implementable in Attestrum (~10 lines), no fork dep needed |
| V5 — fork patch site load-bearing under Option X | **pass** | Stays load-bearing — `materials()` is the only CSR-construction site | No additional fork-extension required for this concern in Session 5 |
| V6 — `attestrum-fingerprint` bundle-byte deps grep | **pass** | Zero matches | No cascade from bundle shape change into FingerprintBundle outputs |
| V7 — cosign Go CSR-subject for absent email | **pass** (with structural note) | N/A — cosign Go has no x509 CSR / no emailAddress attribute at all | Fulcio v2 JSON proof-of-possession wire format; K4 question moot under cosign Go comparison |

---

## 3. V1 details — bundle mediaType + content variant from CI

**Commit**: `f6eff89` (the V1 diagnostic instrumentation commit). The patch inserted between `cosign_interop.rs:139` and `:141` an `eprintln!` block that re-reads `bundle.sigstore.json` after `attest_sign` returns, parses it as `serde_json::Value`, and prints the top-level `mediaType` field + the set of top-level content keys.

**CI run**: `26412678308` (cosign-interop workflow on push of `f6eff89` to `main`).

**Captured chain** (verbatim from the CI log at `2026-05-25T17:40:57Z`):

```
V1_DIAGNOSTIC mediaType=Some(String("application/vnd.dev.sigstore.bundle+json;version=0.2")) content_keys=Some(["mediaType", "messageSignature", "verificationMaterial"])
```

**Interpretation**:
- `mediaType = application/vnd.dev.sigstore.bundle+json;version=0.2` — explicitly Bundle v0.2. Not v0.3.
- `content_keys = ["mediaType", "messageSignature", "verificationMaterial"]` — contains `messageSignature`; does NOT contain `dsseEnvelope`.

**Cross-reference**: this matches `bundle/sign.rs:351-382` (the `SigningArtifact::to_bundle()` method in the cached sigstore-rs at `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/`) which hard-codes both fields:

```rust
Bundle {
    media_type: Version::Bundle0_2.to_string(),
    verification_material,
    content: Some(bundle::Content::MessageSignature(message_signature)),
}
```

**Confidence shift**: ~95% → ~99%. The remaining ~1% is structural-reading uncertainty (e.g., is there a downstream re-write of the bundle JSON before CI captures it?) — Attestrum's `attest_sign` writes the bundle exactly as `to_bundle()` returns it, no post-processing. The CI evidence is consistent with the source-traced shape.

**Decision-gate result**: PROCEED to Commit B. The audit-pack diagnosis is correct; the X→Y hybrid path stays valid; Sessions 1-5 can open after this report lands.

---

## 4. V2 details — cosign Go Rekor entry type — `dsse` (correcting Reviewer 1)

**Source**: `sigstore-go/pkg/sign/transparency.go` (cosign v3 delegates Rekor entry construction to sigstore-go).

**Code path for DSSE-wrapped attestations**:

```go
case dsseEnvelope != nil:
    req = &rekortilespb.DSSERequestV002{
        Envelope:  dsseEnvelope,
        Verifiers: []*rekortilespb.Verifier{verifier},
    }
```

**Imports** confirming both legacy + tiled paths:
- `"github.com/sigstore/rekor/pkg/types/dsse"` — Rekor v1 legacy (v0.0.1)
- `rekortilespb.DSSERequestV002` from `"github.com/sigstore/rekor-tiles/v2/pkg/generated/protobuf"` — Rekor v2 tiled (v0.0.2)

**Entry kind**: **`dsse`** for both Rekor v1 and Rekor v2. The Rekor v2 entry shape is the `DSSERequestV002` protobuf message wrapping the DSSE envelope + verifier list.

**Correction**: Reviewer 1's response (`attestrum-cosign-interop-audit-2026-05-25-reviewer-1-response.md`) hedged toward `Intoto` ("most likely Intoto for attestations"). **This is wrong for cosign v3.** sigstore-go's switch picks `dsse` (not `intoto`) when the artifact is a DSSE envelope, which is the only path cosign uses for `attest-blob` and `sign-blob --bundle`-with-attestation.

**Implication for Session 1**: the sign-flow.md diagram must show:
- Rekor v2 entry kind = `dsse`
- Entry shape = `DSSERequestV002 { envelope, verifiers }`
- The on-disk Bundle's `verificationMaterial.tlogEntries[0].kindVersion.kind = "dsse"` + `.kindVersion.version = "0.0.2"`

**Confidence**: high. The cosign-Go-via-sigstore-go path is unambiguous in the source.

---

## 5. V3 details — no hidden DSSE-sign API in sigstore-rs

**Source 1** — `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/cosign/intoto.rs` (20 lines total):

```rust
// Copyright 2026 The Sigstore Authors.
// Licensed under the Apache License, Version 2.0 (...)
//
//! In-toto Statement v1 types — re-exported from [`crate::bundle::intoto`].
//!
//! The canonical definition lives in `bundle::intoto` so that it is available
//! to the `bundle` feature without requiring the `cosign` feature.

pub(crate) use crate::bundle::intoto::InTotoStatementV1;
```

A 20-line re-export shim. No sign API. Closed.

**Source 2** — `cosign/signature_layers.rs` (2430 lines). All `DsseEnvelope` / DSSE references are verify-side or test-only:
- L418-424: parses `Bundle.content` oneof for `DsseEnvelope` variant (verify decoder).
- L1909-1910: test assertion that verified bundle has `Content::DsseEnvelope`.
- L2184-2187: test fixture decoding.
- L386 / L460-462 / L483-528 / L569-570: DSSE PAE reconstruction + verify (`compute_pae` called from `crate::bundle::verify::models`).

No `fn sign_dsse`, no `fn create_dsse_signature`, no `fn sign_envelope`, no `fn sign_attestation`.

**Source 3** — workspace-wide grep:

```bash
$ rg -n "DsseEnvelope|dsse_envelope|sign_dsse" \
    ~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/
```

Returns only verify-side (`bundle/verify/`, `bundle/verify/models.rs`, `bundle/verify/verifier.rs`) or test usage. Zero sign-side hits. **`bundle/sign.rs` contains zero DSSE-related code.**

**Source 4** — `bundle/sign.rs:351-382` (the `SigningArtifact::to_bundle()` method):

```rust
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
```

Hard-coded `Version::Bundle0_2` mediaType + `Content::MessageSignature` variant. No DSSE branch exists; no feature flag toggles a DSSE path; no alternative constructor.

**Result**: audit §10.1.3 unknown definitively closed. The X→Y hybrid plan does NOT compress to "enable a feature flag" — it stays at "build extractable `dsse_sign` module in `attestrum-attest`, then extract to fork as third commit."

---

## 6. V4 details — sigstore-rs primitives Option X would need

The Option X module (`crates/attestrum-attest/src/dsse_sign.rs`) needs sigstore-rs to:

1. **Build a Fulcio CSR + obtain an ephemeral cert.** Currently in `SigningSession::materials` (`bundle/sign.rs:86`, private — no `pub`).
2. **Sign DSSE PAE bytes with the ephemeral key.** Currently `SigningSession.private_key` (`bundle/sign.rs:68`, private field).
3. **Submit a Rekor v2 `Dsse` entry.** Currently `sign_digest` calls `create_log_entry` with a hard-coded `ProposedLogEntry::Hashedrekord { ... }` at `bundle/sign.rs:164`. No alternative entry-type construction site.
4. **Assemble a Bundle v0.3 with `Content::DsseEnvelope`.** Currently `SigningArtifact::to_bundle` hard-codes `Content::MessageSignature` + `Version::Bundle0_2` at `bundle/sign.rs:351-382`.

**Items currently private / `pub(crate)` that need fork-side `pub` exposure** (8 items total):

| Symbol | Location | Visibility |
|--------|----------|-----------|
| `SigningSession::materials` | `bundle/sign.rs:86` | private fn |
| `SigningSession.private_key` | `bundle/sign.rs:68` | private field |
| `SigningSession.certs` | `bundle/sign.rs:69` | private field |
| `SigningSession.context` | `bundle/sign.rs:66` | private field |
| `SigningArtifact.input_digest` | `bundle/sign.rs:341` | private field |
| `SigningArtifact.cert` | `bundle/sign.rs:342` | private field |
| `SigningArtifact.signature` | `bundle/sign.rs:343` | private field |
| `SigningArtifact.log_entry` | `bundle/sign.rs:344` | private field |
| `bundle::verify::models::compute_pae` | `bundle/verify/models.rs:49` | `pub(crate)` |

**`compute_pae` exception**: trivially re-implementable in Attestrum (~10 lines using `format!("DSSEv1 {} {} {} ", ...)` + `extend_from_slice`). No fork dep needed if Attestrum-side re-implementation is preferred.

**Cleanest fork patch (Reviewer 1's API shape)**: add a single `pub async fn sign_dsse(self, payload_type: &str, payload: &[u8]) -> SigstoreResult<SigningArtifact>` method on `impl SigningSession`. The new method calls `materials()` internally (no field exposure needed) + builds the DSSE PAE + signs with `self.private_key` + posts a Rekor `Dsse` entry (using `DSSERequestV002` shape) + returns a `SigningArtifact` whose `to_bundle()` produces the v0.3-shaped Bundle with `Content::DsseEnvelope`.

This minimises the public-surface footprint to one new method + one optional new field/constructor for `SigningArtifact` (or a separate `SigningArtifact::for_dsse(...)` constructor). It's also the most upstreamable shape — single new public method on an existing public type, idiomatic Rust, easy to review.

**Alternative fork patch (more invasive)**: expose `materials()` + the 3 `SigningSession` fields + the 4 `SigningArtifact` fields + add a `SigningArtifact::for_dsse(...)` constructor. Lets Attestrum keep more of the flow Attestrum-side but is harder to upstream and exposes more surface to other downstream sigstore-rs users.

**Recommendation**: pursue Reviewer 1's shape. The `sign_dsse` method is the minimum-viable fork patch + the most upstreamable PR.

---

## 7. V5 details — fork patch site stays load-bearing

The fork's empty-emailAddress patch lives in `SigningSession::materials` at `bundle/sign.rs:90-114`:

```rust
let subject = vec![
    vec![
        AttributeTypeAndValue {
            oid: const_oid::db::rfc3280::EMAIL_ADDRESS,
            value: AttributeValue::new(
                pkcs8::der::Tag::Utf8String,
                // Workload-identity tokens have no email claim;
                // pass an empty string when absent. Fulcio derives
                // the cert identity from the OIDC token's claims
                // directly (sub/iss), not from the CSR subject,
                // so the empty value here is informational only.
                token
                    .unverified_claims()
                    .email
                    .as_deref()
                    .unwrap_or("")  // ← fork patch
                    .as_bytes(),
            )?,
        }
    ].try_into()?
].into();
```

Under Option X, the new `sign_dsse` method (per V4's Reviewer-1-shape recommendation) calls `materials()` internally as the only CSR-construction site. The patched code remains on the critical path. **No additional fork-extension required** for this concern in Session 5.

**Wire-format confirmation** — `fulcio/mod.rs:205-235` (`request_cert_v2`):

```rust
pub async fn request_cert_v2(
    &self,
    request: x509_cert::request::CertReq,   // ← takes x509 CSR
    identity: &IdentityToken,
) -> Result<CertificateResponse> {
    ...
    let response = client
        .post(self.root_url.join(SIGNING_CERT_V2_PATH)?)
        .headers(headers)
        .json(&CreateSigningCertificateRequest {
            certificate_signing_request: request,  // ← POSTed as wrapped CSR
        })
        .send()
        .await?;
    ...
}
```

sigstore-rs's Fulcio v2 client takes a full `x509_cert::request::CertReq` parameter and POSTs it (JSON-wrapped) to Fulcio's v2 signing-cert endpoint. The x509 CSR is built client-side in `materials()`; the empty-emailAddress patch is *unavoidable* for workload-identity tokens given sigstore-rs's choice to send a full x509 CSR. (Contrast with cosign Go's design — see V7.)

---

## 8. V6 details — `attestrum-fingerprint` bundle-byte deps grep

```bash
$ rg -n "to_bundle|sigstore::bundle::Bundle|attest_sign|sigstore::Bundle|sigstore_bundle|Bundle::" \
    crates/attestrum-fingerprint/src/
crates/attestrum-fingerprint/src/lib.rs:38://! [`FingerprintBundle::iscc`] field (an [`IsccComposition`] of four
```

The single match is a doc-comment reference to Attestrum's own `FingerprintBundle::iscc` type (defined in `attestrum-fingerprint`), NOT the Sigstore `sigstore::bundle::Bundle` type. The grep pattern matched `Bundle::` against `FingerprintBundle::iscc` by suffix.

**Zero references to sigstore bundle bytes** in the `attestrum-fingerprint` source tree. Bundle shape change does NOT cascade into `FingerprintBundle` outputs.

**Reviewer 2's C3 risk discharged.** The X→Y hybrid plan's blast radius stays scoped to `attestrum-attest`.

---

## 9. V7 details — cosign Go CSR-subject for absent email — N/A by design

cosign v3 delegates signing to sigstore-go. sigstore-go's `pkg/sign/certificate.go` uses Fulcio's v2 endpoint via a JSON proof-of-possession wire format (NOT an x509 CSR):

```json
{
  "publicKeyRequest": {
    "publicKey": {"algorithm": "...", "content": "..."},
    "proofOfPossession": "..."
  }
}
```

The Fulcio client struct (`fulcioCertRequest` in sigstore-go) marshals to JSON and POSTs to `/api/v2/signingCert`. **There is NO emailAddress attribute. There is NO x509 CSR. There is NO client-side Subject construction at all.** Fulcio derives the cert subject from the OIDC token's claims server-side (sub / iss).

**Implication for K4 (omit vs empty bytes)**: the question is **moot under cosign Go comparison**. cosign Go has neither — the choice between "omit the attribute" and "pass empty bytes" doesn't exist in cosign Go's design because cosign Go doesn't construct the attribute at all.

**Implication for the fork patch**: sigstore-rs's empty-bytes patch (in `bundle/sign.rs:97-110`) is a sigstore-rs-specific design-choice workaround for sigstore-rs's choice to build an x509 CSR client-side and POST it to Fulcio v2 via `request_cert_v2`. Fulcio v2 accepts both the JSON proof-of-possession shape (cosign Go's path) AND the wrapped x509 CSR shape (sigstore-rs's path). Aligning sigstore-rs with cosign Go's behavior would require switching to the JSON proof-of-possession wire format — far out of V1-V7 scope.

**Recommendation for Session 5**: do NOT pursue cosign-Go-alignment as a fork patch. The empty-bytes approach is structurally appropriate for the x509 CSR wire format sigstore-rs chose. If the founder wants Fulcio v2 JSON proof-of-possession parity, that's a sigstore-rs-architecture-level change deserving its own multi-session plan.

---

## 10. Implications for Session 1 (sign-flow.md diagram update)

The Session 1 sign-flow.md diagram MUST show:

1. **Bundle target shape**:
   - mediaType: `application/vnd.dev.sigstore.bundle+json;version=0.3` (NOT v0.2)
   - Content variant: `dsseEnvelope` (NOT `messageSignature`)
   - `dsseEnvelope.payloadType`: `application/vnd.in-toto+json`
   - `dsseEnvelope.payload`: base64(canonical in-toto Statement JSON bytes)
   - `dsseEnvelope.signatures[0]`: signature over PAE(payloadType, payload) — NOT over `SHA-256(manifest.parquet)` and NOT over `SHA-256(canonical Statement JSON)`

2. **DSSE PAE construction step** (explicit byte-length semantics):
   - `PAE = "DSSEv1 " + len(payloadType) + " " + payloadType + " " + len(payload) + " " + payload`
   - Where `len(x)` is the UTF-8 byte length of `x` (= `x.len()` in Rust for `&str`/`&[u8]`).
   - Pinned test vectors from the DSSE spec at `https://github.com/secure-systems-lab/dsse/blob/master/protocol.md` go into Session 2's unit tests (K3 mitigation).

3. **Rekor v2 entry submission** (corrected from Reviewer 1's hedge):
   - Entry kind: `dsse` (NOT `intoto`)
   - Entry shape: `DSSERequestV002 { envelope, verifiers }` (Rekor-tiles v2 protobuf)
   - Fallback for Rekor v1: legacy `dsse` entry at apiVersion `0.0.1` (`github.com/sigstore/rekor/pkg/types/dsse`) if needed for backward compat

4. **Fork-touchpoint marker**: the diagram should annotate the box where `SigningSession::sign_dsse(payload_type, payload)` is called — that's the fork-side new API (Session 5 introduces it; Session 2 calls it).

5. **Removed boxes** (vs the current sign-flow.md):
   - `SigningSession::sign(input)` call → REMOVED (it's the wrong primitive; it produces MessageSignature)
   - `Hashedrekord` Rekor entry submission → REMOVED (replaced by `Dsse` entry)
   - `MessageSignature` bundle content → REMOVED (replaced by `DsseEnvelope`)
   - Bundle v0.2 mediaType → REMOVED (bumped to v0.3)

6. **Frontmatter for the new sign-flow.md revision**:
   - `source_of_truth: diagram` during Session 1 (the diagram is the contract code must implement; Session 2 implements against it).
   - Flip to `source_of_truth: code` after Session 4 stabilises + cosign-interop CI goes green.
   - `last_verified` SHA: whatever the Session 1 commit SHA is.

7. **`crates/attestrum-attest/src/sign.rs:99-101` comment sweep** (Reviewer 1's Step 1, deferred to Session 1's prep commit): the existing comment claims "Builds the DSSE envelope" — currently false. Either delete the comment (if Session 1's planned implementation immediately replaces the false comment with a true one) or update it to describe the actual `MessageSignature`-producing behavior (if Session 2 is the implementation commit and Session 1 is diagram-only). Founder's call — surface in Session 1's plan-mode.

---

## 11. Risks discovered

1. **Reviewer 1's `Intoto` hedge was incorrect.** V2 found cosign Go (via sigstore-go) submits `DSSERequestV002` entries (kind = `dsse`), not `Intoto`. Session 1's diagram must show the `dsse` kind. Reviewer 1's response document is otherwise sound — this single hedge was the only divergence from source-of-truth and is now corrected. Future external reviewers writing in the same general area of the Sigstore ecosystem should be assumed-similarly-fallible on Rekor-entry-type details unless they cite specific source files.

2. **The fork's empty-bytes-emailAddress patch is sigstore-rs-specific.** V7 surfaced that cosign Go doesn't have the same problem because it uses Fulcio v2's JSON proof-of-possession wire format. The audit-pack's K4 ("we should ALSO change the fork's empty-bytes to OMIT") action is **withdrawn** as a Session 5 follow-up — there's no cosign Go behavior to align with. If the founder wants Fulcio v2 JSON proof-of-possession parity for architectural reasons, that's a separate multi-session sigstore-rs-architecture change, not a Session 5 task. Surfaced for awareness.

3. **The `bundle::verify::models::compute_pae` function exists at `pub(crate)`.** V4 noted Attestrum can re-implement DSSE PAE in ~10 lines locally without fork dep — but a Session 5 nice-to-have would be to expose `compute_pae` as `pub fn` in the fork to keep the fork API surface consistent (sign + verify both expose PAE). Low priority; defer to Session 5 author's judgment.

4. **K4 + K10 from the decision doc are downgraded.** K4 (omit-vs-empty-bytes patch follow-up) is moot per V7. K10 (anchor-bias sweep of `~/.claude/plans/`) is out of scope for this cycle but worth a future pass.

5. **No new architectural surprises.** V3 confirmed there is no hidden API; V6 confirmed no cascade into `attestrum-fingerprint`; V5 confirmed the fork patch stays load-bearing. The X→Y hybrid plan as written in the decision doc is structurally correct and unchanged.

6. **CI state at report-write**: the V1 commit (`f6eff89`) triggered the cosign-interop workflow as expected. cosign-interop = failure (as expected — no fix landed); the V1 diagnostic eprintln appeared at the expected line in the failure log. `ci` workflow + `determinism` workflow status not separately observed in this report — assumed green per the pre-V1 pattern; surface to founder if a green→red regression appears on `f6eff89` for either of those workflows.

7. **Tokenmaxxen draft candidates surfaced during this cycle**:
   - "sigstore-rs 0.14.0 has no DSSE-attestation sign API anywhere in the public OR `pub(crate)` API surface; `SigningSession::sign` is the only sign entry point and it produces `MessageSignature` + `Bundle v0.2`." Could save another agent ≥30 min if they're writing in-toto attestation code against sigstore-rs.
   - "cosign Go (via sigstore-go) uses Rekor v2 `DSSERequestV002` entries (kind = `dsse`) for DSSE attestations, NOT `Intoto` entries. Reviewer-1-class hedges toward `Intoto` are wrong for cosign v3." Useful for anyone integrating with Rekor v2.
   - "cosign Go uses Fulcio v2's JSON `publicKeyRequest + proofOfPossession` wire format — there is NO x509 CSR and NO emailAddress attribute in cosign Go's signing path. sigstore-rs uses a different (x509 CSR) wire format against the same Fulcio v2 endpoint." Useful for anyone hitting the empty-vs-omit-emailAddress question.
   - Per CLAUDE.md global tokenmaxxen draft-mode anchor, drafts can land via `log_learning(publish_mode: "draft")` without auto-publishing. **NOT drafted in this cycle** per the handoff §8 constraint #13 — founder hasn't OK'd. Founder can ask for the drafts to land if useful.

---

## 12. Next steps after this report

1. **Commit C of this cycle**: revert the V1 diagnostic eprintln in `crates/attestrum-attest/tests/cosign_interop.rs`. Keeps `main` clean before Session 1. Five-gate + CHANGELOG + SESSION-LOG + push.

2. **Plan-mode Session 1 (sign-flow.md diagram update)**: per CLAUDE.md §5 diagram-first gate. The next agent reads this report's §10 + the decision doc §6 Session 1 description, drafts the updated `docs/diagrams/sprint-4/sign-flow.md`, surfaces the comment-sweep question for `crates/attestrum-attest/src/sign.rs:99-101`, exits plan mode for founder approval.

3. **Sessions 2-5**: implement the X→Y hybrid per decision doc §6. Session 2 builds the extractable `dsse_sign` module in Attestrum. Session 3 confirms `attest_verify` semantics. Session 4 ships cosign-interop tests + verification assertions. Session 5 extracts `dsse_sign` to the fork as a third commit + bumps `[patch.crates-io]` rev.

4. **Session 6 (out-of-band)**: upstream sigstore-rs PR for the `sign_dsse` method. Async multi-week; doesn't block Sprint 5.

5. **Sprint 5 S5-D1 E5 timing decision (founder)**: pause until cosign-interop is green vs continue in parallel? All three audit-pack reviews recommend pause; decision doc §8 surfaces it; founder's call.

---

## 13. Verification report status

✅ All seven V-checks complete. All seven results captured. Implications for Session 1 enumerated. Risks discovered logged.

**Recommendation**: open plan-mode Session 1 after Commit C lands.

---

*Report written 2026-05-25 by Claude Opus 4.7 (1M context). V2-V7 source traces conducted against `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/` + raw.githubusercontent.com fetches of cosign + sigstore-go main branches as of the same date. V1 CI evidence from run `26412678308` at commit `f6eff89` captured at `2026-05-25T17:40:57Z`. If anything below contradicts subsequent source-of-truth changes (e.g., sigstore-rs / sigstore-go / cosign code moves), the subsequent source wins — surface contradictions to founder before relying on this report's claims for new commits.*
