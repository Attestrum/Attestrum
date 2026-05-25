# Reviewer response: Attestrum cosign-interop verify-side audit

## 2026-05-25 — Claude (web/chat, Opus 4.7)

**Role**: independent reviewer per §1 of the audit doc.
**Audit reviewed**: `attestrum-cosign-interop-audit-2026-05-25.md` at repo HEAD `d3e352b`.
**Reading depth**: full document (§§1–12); cross-referenced against the audit's quoted sigstore-rs source spans. Did not pull the Attestrum repo or sigstore-rs fork directly — this is a desk review of the evidence the audit author presents.

---

### TL;DR

1. **§6 diagnosis is correct.** Verdict: confirm, with a reframing in §A below that I think is sharper than "signed hash ≠ verified hash".
2. **Recommend Option X**, executed as an explicit **X → Y hybrid**: build the DSSE machinery as a *cleanly extractable* module inside `attestrum-attest` so the eventual upstream PR to sigstore-rs is a code-move, not a rewrite.
3. **Reject Option Z** (acquirer-hostile, violates PATH-A-BRIEF §1.5 and §12). **Reject Option W** (abandons Rust-only thesis; cosign Go's nondeterminism conflicts with the §7 byte-identity invariant).
4. **Timing: land before S5-D1 E5** (the API-freeze + cross-target byte-determinism gate). Freezing the wrong bundle shape costs more than the schedule slip.
5. **Seven risks the author didn't fully enumerate** in §C below. Two are decision-relevant before the next plan-mode session starts.

---

### A. Diagnosis verification (§6)

**Agree the diagnosis is correct.** The evidence chain the author presents is internally consistent and the inference is forced:

- `SigningSession::sign_digest` (sign.rs:144–199) signs `SHA-256(input)` with the ephemeral ECDSA-P256 key and emits a `Hashedrekord` Rekor entry. Confirmed by the quoted source.
- `SigningArtifact::to_bundle()` (sign.rs:351–382) hard-codes `Content::MessageSignature` + `Version::Bundle0_2`. The `dsseEnvelope` field is never populated on the sign path.
- `Verifier::verify_bundle_content` (verifier.rs:62–83) for the `MessageSignature` arm calls `verify_prehash(signature, input_digest)` where `input_digest = SHA-256(input)` and `input` is whatever bytes the caller passes to `Verifier::verify`.
- Attestrum's `attest_sign` passes `req.statement_payload` (the in-toto Statement canonical-JSON bytes) as the signed input. Attestrum's `attest_verify` passes the **`manifest.parquet` file handle** as the verify input. These are different byte streams. Ring rejects. `PublicKeyVerificationError` is the inevitable result.

The author's 15% doubt window mostly evaporates on closer reading of their own evidence: the grep across `Content::Dsse | Content::DsseEnvelope | Dsse | in_toto | InToto` on the sign path returning zero construction sites is a strong negative result. The two residual unknowns (`cosign/intoto.rs` deep-read; unreleased 0.15.x) are worth verifying for completeness but **shouldn't block decision-making** — the cosign module is disabled by your `default-features = false` config, and waiting for an unreleased version would be Option Y's worst-case path anyway.

**Reframing I think is sharper than "signed hash ≠ verified hash":**

> `SigningSession::sign(blob)` is a **blob-signing primitive**: it produces a Bundle that attests to "these bytes". Attestrum is misusing it as an **attestation-signing primitive**: it wants a Bundle that attests to "this Statement, which itself attests to these other bytes". sigstore-rs 0.14.0 simply doesn't expose the latter on the public sign path. The signed/verified-byte mismatch is the *symptom*. The *cause* is a missing API.

This framing matters because:
- It explains why Option X is architecturally correct (build the missing API), not just pragmatic.
- It explains why Option Z is wrong shape (it papers over the missing API by changing what `attest_verify` means, which then breaks cosign-Go interop forever).
- It makes the upstream PR pitch for Option Y crisper: "sigstore-rs needs a `sign_dsse(payload_type, payload)` API; here it is."

**On question §10.1.3** (did the author miss anything in the disabled `cosign/` module): worth a 30-minute grep in the next plan-mode session, but I'd weight it < 10% likely to change the diagnosis. `cosign/intoto.rs` is verify-side OCI-layer parsing in the broader sigstore-rs source layout; there's no precedent for sigstore-rs hiding a public sign API behind a non-default feature gate.

---

### B. Option ranking and recommendation

**Recommended: Option X, executed as an explicit X→Y hybrid.**

Concretely:

1. Build the DSSE construction logic as a **new, internally well-isolated** module in `attestrum-attest` — call it `dsse_sign` or `attest_dsse`. The module's public surface is `sign_dsse(ctx: &SigningContext, id_token: &IdentityToken, payload_type: &str, payload: &[u8]) -> Result<Bundle, _>`. **No Attestrum-shaped types in the signature.**
2. Inside the module: construct the DSSE PAE bytes, obtain the Fulcio cert via sigstore-rs's lower-level CSR primitives (likely requires a third commit on the existing fork branch to expose currently `pub(crate)` items — that's fine, the fork is already an approved git-pinned dep per Sprint 5 Q1), sign the PAE, build the Bundle v0.3 protobuf-JSON with `Content::DsseEnvelope`, submit the right Rekor v2 entry type.
3. `attest_sign` becomes a thin caller: build the in-toto Statement, canonical-JSON-serialize, call `dsse_sign("application/vnd.in-toto+json", statement_bytes)`.
4. After it works, the upstream PR to sigstore-rs (Option Y proper) is a **code-move**, not a re-implementation. The fork's third commit becomes the upstream PR. If sigstore-rs accepts it, Attestrum cuts over to upstream and the fork drops to its current 2-commit JWT-parse-only shape, or is eliminated entirely if those commits also land upstream.

**Why this beats straight X**: it captures Y's strategic upside (ecosystem contribution, acquirer narrative, fork eventually drops) on X's timeline. The author's recommendation in §8 already gestures at this; I'm formalizing the architectural discipline that makes the eventual upstream extraction trivial rather than a refactor.

**Why this beats straight Y**: upstream PR review latency on sigstore-rs is unbounded (multi-month is plausible for an API addition). Sprint 5 cannot block on that. You ship X today, you offer Y to the upstream community in parallel, you accept whatever timeline they want.

**Why not Z**: acquirer-hostile. PATH-A-BRIEF §1.5 ("cosign verify-blob-attestation returns exit 0 + Verified OK") and §12 ("substrate, not a branded silo") are load-bearing pitch elements for Path A's AI2/Pleias/EleutherAI/HF customer set. These are organizations that will *test* the cosign-interop claim. Failing that test post-pivot is a credibility-fatal outcome that 1 day of saved engineering doesn't justify.

**Why not W**: three independent reasons, any of which is sufficient:
- Cosign Go's bundle output is not byte-deterministic across runs (RFC 6979 status of ECDSA k-nonces varies by build; timestamps embedded in Rekor responses). Determinism is a CLAUDE.md §7 invariant and a PATH-A-BRIEF differentiator. W loses it.
- Subprocess invocation is an explicit BUILD-PLAN §6.2 anti-pattern.
- Acquirer story is materially weaker. "Attestrum is a cosign Go wrapper" is a smaller acquisition target than "Attestrum is a Rust-native Sigstore consumer that upstreamed missing pieces to the SDK".

**On §10.2.6** (is there a 5th option): no, I don't think so. The candidate I'd consider — "use sigstore-rs's lower-level Fulcio + Rekor primitives but assemble the Bundle ourselves" — collapses to Option X. Building a Sigstore client from scratch or pivoting away from Sigstore both violate stated goals.

---

### C. Risks the author may have missed or under-enumerated

In rough order of decision-relevance for the next plan-mode session:

**C1. Rekor v2 entry type choice: `Intoto` vs `Dsse`.** The audit flags this in passing (§8 Option X step 4) but doesn't pick. **This is decision-relevant before any code lands.** Cosign Go's `attest-blob` emits `Intoto`-flavored Rekor entries for in-toto attestations, not `Dsse`. If Attestrum emits a `Dsse` entry, cosign-Go verify may reject the bundle with a different error than the current one, and you'll have to land *another* round of changes. Verify the canonical choice against `cosign verify-blob-attestation`'s source before committing.

**C2. The fork's `ade5422` empty-emailAddress patch site may be off the new code path.** Option X plans to use sigstore-rs's lower-level Fulcio CSR primitives. The fork's patch is at `sign.rs:97–110`, modifying CSR-subject construction inside the high-level signing flow. **If the lower-level primitives bypass that code path, the workload-identity OIDC bug returns at sign time** under Option X — and the failure mode looks different from today's, so it may not be obvious. The plan-mode session must verify that the patch site is on Option X's code path, or fork-extend the patch to cover the new path.

**C3. `attestrum-fingerprint::FingerprintBundle` may include bundle bytes in hash inputs.** Author flags this as a question in §10.4.9 but doesn't enumerate. If `FingerprintBundle` includes the Sigstore bundle's serialized bytes in any of its hash inputs (or in the manifest), bundle-shape change cascades into fingerprint output changes. **5-minute grep** in the plan session: `rg -n "to_bundle|Bundle|sigstore" crates/attestrum-fingerprint/src/`.

**C4. DSSE PAE byte-determinism is bounded-risk but cross-target-relevant.** Author flags this as Option X's #2 con. To de-risk: the DSSE spec ships canonical test vectors. Build a unit-test suite that pins:
- PAE byte string for `(payload_type, payload)` pairs from the DSSE spec's test vectors.
- Length encoding (decimal ASCII, no leading zeros, single space delimiters).
- UTF-8 byte-length vs character-length distinction (the spec is byte-length; trivial to get wrong).
Also: extend the cross-target determinism harness (§7.4 mentions S5-D1 E5 is the byte-identity gate) to compare the **Rekor entry bytes**, not just the bundle bytes — Rekor entry assembly is the larger nondeterminism risk surface.

**C5. `sign-flow.md` (and `verify-flow.md`) diagram update is a CLAUDE.md §5 hard gate.** Audit mentions this in passing in §11.1. Worth being explicit: the bundle-shape change requires diagram updates *before* any production code lands. The diagram-linter strict mode at the five-gate set will block the commit otherwise. Plan the diagram commit as commit 1 of the landing sequence.

**C6. Sprint 6 `verify.html` deliverable's bundle-parsing surface area.** Audit flags as §10.4.9; worth explicit treatment. If `verify.html` is a static-page verifier (which Sprint 6 PATH-A-BRIEF deliverable implies), its bundle-parsing logic must be designed against Bundle v0.3 + DSSE-envelope shape, not v0.2 + MessageSignature. **Surface this in the next Sprint 6 plan** so it's not designed against the soon-to-be-obsolete v0.2 shape.

**C7. Bundles already emitted in test runs / determinism goldens are now known-bad.** Any cosign-interop run pre-X-fix, plus any bundles checked into golden tests, are MessageSignature-v0.2-shaped and will need re-issuance under the new shape. The plan-mode session should enumerate which golden directories are affected and plan a single re-issuance commit.

---

### D. Answers to specific questions in §10 where I have a view

| Q | View |
|---|------|
| **10.1.1** Is §6 correct? | Yes. ~95% confidence on the evidence presented. |
| **10.1.2** Is the framing right? | The "signed-hash ≠ verified-hash" framing is correct but symptomatic. See §A above for the stronger reframing ("missing API, not byte mismatch"). |
| **10.1.3** Did the author miss anything in disabled `cosign/`? | Worth a 30-min grep; <10% likely to change the diagnosis. |
| **10.2.4** Which option? | X executed as X→Y hybrid. See §B. |
| **10.2.5** Is "X with eventual Y upstreaming" right? | Yes, but make the architectural discipline explicit (extractable module surface) so the eventual upstream PR is a code-move not a refactor. |
| **10.2.6** 5th option? | No. |
| **10.3.7** When? | Before S5-D1 E5. Freezing the wrong bundle shape mid-sprint is more expensive than the schedule slip. |
| **10.3.8** Does the fork's `60af47e` JWT-parse patch survive? | Yes for X / Y / Z. For W it becomes dead code. Either way the patch is a real and correct fix for a real bug. Keep it. |
| **10.4.9** Sprint 5 deps the author missed? | C3 (fingerprint), C6 (verify.html), C7 (golden re-issuance). See §C above. |
| **10.4.10** Article 53 golden affected? | Almost certainly yes via C7. Re-issue under X. |
| **10.4.11** Determinism risks? | Bounded but real. See C4. PAE byte assembly + Rekor entry serialization are the surfaces. |
| **10.5.12** Loop in founder before/after planning cycle? | Before. The audit itself is the surface-to-founder gate; the founder picks X/Y/Z/W; the next plan-mode session executes the chosen option. Don't start planning until Austin has picked. |

---

### E. Suggested next actions for the founder

1. **Read §A and §B**. If the reframing and the X→Y hybrid pitch land, you have your answer.
2. **Greenlight Option X (X→Y hybrid framing)** to the next Claude Code agent, OR push back on this review if any of §C1–C7 changes your reasoning.
3. **Next plan-mode session opens with**:
   - A `docs/diagrams/sprint-4/sign-flow.md` update (commit 1, diagram-first per §5).
   - A 30-min verification grep of `cosign/intoto.rs` to close the §10.1.3 unknown.
   - A grep of `attestrum-fingerprint` for bundle-byte dependencies (C3).
   - A plan for the `dsse_sign` module's public surface designed for eventual upstream extraction.
   - A plan for the fork's third commit (exposing whatever lower-level Fulcio CSR primitives Option X needs) and verification that the empty-email patch site stays on the active code path (C2).
   - A plan for golden re-issuance under the new bundle shape (C7).
   - An explicit pick on Rekor v2 entry type (`Intoto` vs `Dsse`) verified against cosign Go's source (C1).
4. **Surface a parallel-track decision**: do you also want an upstream sigstore-rs PR to land in parallel (Option Y as a separate workstream), or does that wait until X has shipped and proven stable? My weak preference: file the upstream PR as soon as the fork-third-commit code is stable, even before Attestrum cuts over, so the upstream review clock starts early.

---

*End reviewer response. The diagnosis holds, the option matrix is sound, X→Y hybrid is the recommendation, seven risks (C1–C7) warrant explicit treatment in the next plan-mode session. Two of those — C1 (Rekor entry type) and C2 (fork patch site) — should be resolved before code lands, not after.*
