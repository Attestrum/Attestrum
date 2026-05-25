# Self-review of `attestrum-cosign-interop-audit-2026-05-25.md`

**Reviewer**: Claude Opus 4.7 (1M context) — **same agent that wrote the audit doc**. This is a self-critique drafted at the founder's request so that incoming external reviewer responses can be compared against the author's own second pass.

**Date**: 2026-05-25
**Position relative to the audit**: Companion file, repo root.
**Bias acknowledgment**: This is not an independent review. The author is grading their own work. The honest thing to do is be HARDER on the original diagnosis than the audit was — the value of the exercise is finding what's wrong, not validating it. Reviewers reading this should treat it as the author's best attempt to attack their own conclusions, then judge for themselves.

**Bottom line up front**: the §6 diagnosis is *probably* right but the audit's 85% confidence is over-stated for a structural-bug claim. There is a cheap (~30 min) verification step that should run BEFORE committing to any of options X/Y/Z/W. The author also missed a 5th option (hybrid — cosign Go for sign, sigstore-rs for verify), and the audit's "1-3 days for Option X" estimate is probably too tight by 2x.

---

## 1. Reading instructions

This file mirrors §10 ("Specific questions for reviewers") from the audit. For each numbered question the author has a position; this self-review states it, then attacks it.

Audit reviewers who are NOT the author should not read this file before forming their own opinion — it'll bias your independent read. Read the audit first, draft your own response, THEN compare to this. The whole point is triangulation.

---

## 2. Diagnosis verification (audit §10 Q1-Q3)

### Q1: Is the §6 diagnosis correct?

**Author's position in the audit**: ~85% confident the bug is "sigstore-rs 0.14.0 has no DSSE sign API on the public path → Attestrum signs a MessageSignature bundle → verify-side hashes the wrong byte stream → mismatch".

**Self-attack**:

The 85% number is squishy. For a structural sign/verify mismatch this clear-cut, real confidence should be either ~99% (if proven by inspecting the actual emitted bundle JSON) or ~60% (if there's a real chance the author missed an API in sigstore-rs). 85% is the author hedging without committing.

What would convert 85% → 99% in <30 minutes:

1. **Re-trigger the cosign-interop CI workflow with a debug instrumentation patch** that prints the bundle's `content` variant (`messageSignature` vs `dsseEnvelope`) and `mediaType` (`Version::Bundle0_2.to_string()` vs `Version::Bundle0_3.to_string()`) before the verify call. Direct evidence. The Step 1 commit already established the pattern of "instrument, push, read CI log".
2. **OR**: download the bundle file from a successful sign step (the test panics at verify, AFTER `attest_sign` returns successfully and the bundle has been written to disk). The bundle path is `tmpdir.join("bundle.sigstore.json")`. If we add `eprintln!("BUNDLE_JSON_FOR_INSPECTION: {}", std::fs::read_to_string(&bundle_path)?)` before the verify call and re-push, the CI log shows us the actual bytes.

Both of these are concrete actions that would close the 15% gap. The audit should have proposed them as a Step 1.5 (between today's Step 1 and the next option-X-or-Y planning cycle). The audit didn't — that's a real gap.

**Refined position**: 85% diagnosis confidence + concrete-cheap-step to push to 99% before committing to a fix. The author SHOULD NOT recommend Option X/Y planning cycles start until this verification is done.

### Q2: Is the "signed hash ≠ verified hash" framing right?

**Author's position in the audit**: yes, the framing is right — `attest_sign` signs `SHA-256(canonical statement JSON)` via sigstore-rs's `session.sign(statement_payload)`; `attest_verify` passes `manifest.parquet` to `verifier.verify`, which hashes IT, producing a different digest.

**Self-attack**:

This framing is *technically* correct but maybe not the most useful one. Three alternative framings to consider:

**Alt-framing A**: "sigstore-rs's API contract leaks". `SigningSession::sign(input)` accepts arbitrary bytes; `Verifier::verify(input, ...)` also accepts arbitrary bytes; sigstore-rs assumes the caller passes the SAME bytes to both. The bug is that Attestrum violates this contract by passing different byte streams. Under this framing, the "fix" is: pass the same bytes to both.

**Alt-framing B**: "Attestrum's mental model is two-layer (bundle attests to Statement; Statement attests to manifest), but the chosen sigstore-rs API is one-layer (bundle attests directly to input bytes)". Under this framing, the "fix" is: either commit to the one-layer model (Option Z) OR build the two-layer model on top (Options X/Y/W).

**Alt-framing C**: "Attestrum was written against a documented behavior that doesn't exist in sigstore-rs 0.14.0". Under this framing, the fix is: surface the doc-vs-code mismatch upstream + decide whether to implement what the doc said or change the doc.

The author picked the byte-stream framing because it points directly at the test failure. But Alt-framing B is probably the most useful for the founder's decision because it makes Z's tradeoff clearer ("you're abandoning the two-layer mental model"). The audit should have surfaced Alt-framing B more explicitly.

**Refined position**: byte-stream framing is correct and diagnostic. Alt-framing B is the right framing for the decision in §8. The audit conflated them.

### Q3: Did the author miss anything in sigstore-rs's `cosign/` module?

**Author's position in the audit**: probably not — the module is `default-features = false`-disabled in workspace `Cargo.toml`, and Attestrum doesn't import it.

**Self-attack**:

This is the weakest part of the audit. The author admitted in §6.7 to NOT deep-reading `src/cosign/intoto.rs` or `src/cosign/signature_layers.rs`. The grep result showed both files reference DSSE. That's not "probably nothing" — it's "the author didn't look".

The honest move would have been: spend 10 of the 30-minute investigation budget reading those two files. The author chose not to. Reviewers should consider this an open question, not a settled one.

There's a reasonable chance one of those modules has a DSSE-aware `cosign_sign` function the author missed. If so:
- Option X becomes "enable a sigstore-rs feature flag + call the existing function" — much smaller scope.
- The fork-vs-upstream story might collapse entirely.

**Refined position**: this is a real gap. The next investigation cycle (Step 1.5 above) should also deep-read `src/cosign/intoto.rs` and `src/cosign/signature_layers.rs`. Before committing to X/Y/Z/W, the author wants to be sure these aren't a free Option X.

---

## 3. Option ranking (audit §10 Q4-Q6)

### Q4: Which of X / Y / Z / W do I recommend?

**Author's position in the audit**: Option X with eventual Y upstreaming.

**Self-attack**:

The audit ranked X over Y on timeline ("upstream PR latency is a real risk to Sprint 5 timelines"). That's defensible but assumes Sprint 5 actually has a hard timeline gate that cosign-interop blocks. Does it?

Looking at the audit's §7.4:
- S5-D1 E5 (API freeze + byte-determinism gate): only loosely depends on bundle shape — the byte-identity comparison works against whatever shape we emit.
- Sprint 5 E11 (cosign-interop tests for proof predicates): depends on training-corpus cosign-interop being green first.
- `attestrum prove` workflow: depends on bundle shape because the proof predicates use the same bundle infrastructure.

So Sprint 5 E11 is the actual blocker. E11 is a late-Sprint-5 deliverable. If Option Y's upstream PR review takes 2-3 weeks (median for sigstore-rs given upstream maintainer cadence), it MIGHT land in time for E11 anyway. The audit's "X over Y for timeline" framing is therefore not as solid as it sounded.

**Counter-counter-attack**: even 2-3 weeks of upstream-PR latency is risky. If the PR is rejected or needs major rework, we eat the fork burden indefinitely. Option X is bounded — we know how to ship it. Option Y has an unknowable tail.

**Refined position**: still X, but with more humility about Y's value. If the founder is willing to take the upstream-PR risk for the ecosystem fit + acquirer narrative, Y is genuinely better. The audit's X-recommendation is a "safe default", not a "clearly correct" choice.

### Q5: Is the audit's X-then-Y-upstreaming framing right?

**Author's position in the audit**: yes — build it in Attestrum first, then upstream the cleaned-up version to sigstore-rs.

**Self-attack**:

This is a polite version of "do it ourselves, donate later". The "donate later" half rarely happens in solo-founder projects because once the code works locally, the incentive to clean it up for upstream evaporates. The audit's recommendation should either:
- Commit to NOT upstreaming and own that decision openly (smaller acquirer narrative, faster shipping).
- Commit to upstreaming on a timeline (e.g., "X for v0.1; Y as a v0.2 follow-up before Sprint 6") and write the commitment down.

The wishy-washy "X with eventual Y" is the worst of both worlds — it's the kind of phrasing that produces 18 months of fork maintenance.

**Refined position**: pick one — X-and-NOT-upstream, OR Y-from-day-one. The audit ducked this choice.

### Q6: Is there a 5th option?

**Author's position in the audit**: probably not, but reviewers should look.

**Self-attack**:

There IS a 5th option the audit missed: **Option V — Hybrid: cosign Go for sign-side, sigstore-rs for verify-side**.

Mechanic: replace `attest_sign`'s call to `session.sign(statement_payload)` with a subprocess call to `cosign sign-blob --attestation` (or equivalent — cosign's exact subcommand for "sign in-toto Statement and emit Bundle v0.3" needs verification). Cosign Go writes the DSSE Bundle v0.3 to disk. Attestrum reads the bundle, returns the path. The verify side stays as it is — sigstore-rs's `Verifier::verify` already correctly handles DSSE Bundle v0.3.

This is NOT the same as Option W (which proposed cosign Go for both sign AND verify). Option V is more targeted:
- Solves the bug (DSSE bundle gets emitted).
- Keeps the verify path in-process Rust (which is the surface the founder cares more about — it's the headline "cosign-verifiable bundles, no Attestrum install needed" promise).
- Avoids re-implementing DSSE PAE construction (Option X's main risk).
- Doesn't depend on upstream PR latency (Option Y's main risk).

**Pros (relative to X)**: faster, fewer determinism risks, no upstream PR.

**Cons (relative to X)**: cosign Go binary becomes a build-time + sign-time dependency. Cross-platform friction (must distribute cosign or document the install). Loses some of the "single deterministic Rust CLI" thesis on the sign path only.

**Cons (relative to W)**: still has cosign Go binary dep on sign side, but verify stays pure Rust.

Effort estimate: ~1-2 days. Comparable to Option W but smaller scope on the verify side.

**Refined option matrix**:

| Option | Effort | Restores cosign-interop | Architectural impact |
|--------|--------|------------------------|----------------------|
| **X** — Build DSSE in `attest_sign` | 1-3 days (probably 3-5) | Yes | Medium |
| **Y** — Upstream sigstore-rs sign_dsse | 2-4 days fork; 2-3 weeks upstream | Yes | Low |
| **Z** — Accept MessageSignature | ~1 day (probably 2) | NO | Smallest |
| **W** — Shell out to cosign Go for both | 1-2 days | Yes | Largest |
| **V** — Cosign Go sign, sigstore-rs verify (NEW) | 1-2 days | Yes | Medium-small |

The audit missed V. That's a real gap. V is probably the BEST option for "ship cosign-interop fast without the upstream-PR risk and without the in-process determinism risk of X". The author would now refine the recommendation to:

**Refined recommendation**: V first to unblock Sprint 5 E11, X-or-Y as a follow-up if/when the founder wants the pure-Rust thesis back.

### Effort estimates self-attack

The audit's "1-3 days" for Option X is probably too tight. Breaking down what X actually requires:

1. DSSE PAE byte-encoder (with tests covering UTF-8 length vs byte length, edge cases): 4-8 hours.
2. Low-level Fulcio CSR + ephemeral key generation (extracted from sigstore-rs's internal flow, possibly requires fork patches): 4-12 hours.
3. DSSE signing via `p256::ecdsa::Signature` over the PAE bytes: 1-2 hours.
4. Rekor v2 DSSE-entry submission (different entry type than the current `Hashedrekord`): 4-8 hours.
5. Bundle v0.3 protobuf-JSON assembly with `Content::DsseEnvelope`: 2-4 hours.
6. Verification material assembly (cert chain, inclusion proof, integrated time): 2-4 hours.
7. Test updates (cosign_interop test + new unit tests for each new code path): 4-8 hours.
8. Diagram updates (sign-flow.md, possibly verify-flow.md): 1-2 hours.
9. CHANGELOG + SESSION-LOG entries: 30 min.
10. Iterating against CI (Option X probably needs 2-4 push cycles to get right): 4-8 hours of waiting + diagnosing.

Total: **~30-60 hours of focused work**. That's 4-8 days, not 1-3. The audit was optimistic.

**Refined position**: X effort estimate should be 4-8 days, not 1-3. Option V's "1-2 days" is more credible because the subprocess-orchestration code is well-understood.

---

## 4. Timing and Sprint 5 implications (audit §10 Q7-Q8)

### Q7: When should the fix land?

**Author's position in the audit**: open question.

**Self-attack**:

The audit ducked answering this. A self-review should commit.

Sprint 5 has:
- E5: API freeze. The bundle shape is part of the API, so E5 SHOULD know the final shape. If we don't fix cosign-interop pre-E5, E5 freezes a wrong shape and we re-freeze later.
- E11: cosign-interop for proof predicates. Hard-blocked on training-corpus cosign-interop being green.

The right answer is: **pre-E5**. Fixing now and re-doing E5 around the correct bundle shape is much cheaper than freezing the wrong shape and reverse-engineering later.

But: if Option V is the choice (cosign Go for sign), pre-E5 timing is easy — 1-2 days inserts before E5.

If Option X is the choice (build DSSE ourselves), pre-E5 timing is tighter — 4-8 days might push into E5's window.

If Option Y is the choice (upstream PR), pre-E5 is probably impossible.

**Refined position**: timing pressure favors Option V over X over Y. The audit didn't connect this dot.

### Q8: Should the fork's `60af47e` JWT-parse patch survive?

**Author's position in the audit**: yes for all four options.

**Self-attack**:

Mostly right, but with a twist: if Option V (cosign Go for sign) is chosen, the JWT-parse patch becomes unused on the sign path (cosign Go handles workload-identity OIDC natively). But sigstore-rs is still used on the verify side; does it parse OIDC tokens during verify?

Quick mental check: verify reads OIDC identity from the bundle's leaf cert (via `IdentityToken::try_from` on the cert's SAN extension? Or via direct cert parsing?). If verify uses the `IdentityToken` type at all, the JWT-parse patch is still load-bearing.

The author hasn't actually verified this. It's a question to settle if V is chosen.

**Refined position**: under Options X/Y/Z/W, the JWT-parse patch survives. Under Option V, *probably* survives but needs verification. The audit overpromised.

---

## 5. Risks the audit may have missed (audit §10 Q9-Q11)

### Q9: Sprint 5 entanglements

**Author's position in the audit**: listed §7.4 entanglements but probably undersold.

**Self-attack**:

The audit listed:
- attestrum-pipeline (manifest construction): doesn't depend on bundle shape — manifest is independent.
- attestrum-fingerprint (`FingerprintBundle`): doesn't depend on bundle shape — fingerprint outputs aren't Sigstore bundles.
- `verify.html` Sprint 6 deliverable: depends on bundle shape.
- Dataset card README rendering: depends on bundle shape.

The audit missed:
- **Determinism golden tests** for the bundle. Cross-target byte-identity is asserted across linux-x86/aarch64, macos-aarch64, linux-musl. If the bundle shape changes, all four targets' golden bundles change. The golden tests don't exist yet (E5 ships them) but if any LATER E-commit asserts against goldens that were generated under the wrong shape, they'd need regeneration. Surface area: every E-commit between today and Sprint 6.
- **The Article 53 EU template** (PROTECTED per CLAUDE.md §4). The Article 53 golden tests assert specific bundle field shapes. The audit said "Author thinks no" — but didn't actually grep for it. A real reviewer would check.
- **The dataset card README rendering** — the audit listed this but didn't quantify. If the README embeds a snippet of the bundle JSON for transparency display, the snippet's shape (DSSE envelope vs MessageSignature) is what end-users see. That's a doc / UX concern even if the cryptographic semantics are fine.

**Refined position**: Sprint 5 + Sprint 6 entanglements are real and the audit undersold them by ~30%. A bundle-shape change has reach into goldens, docs, README rendering, and the verify.html surface.

### Q10: Article 53 golden tests

**Author's position in the audit**: probably not affected.

**Self-attack**:

The author guessed. Reviewers should grep `tests/golden/article53/` for "messageSignature" / "dsseEnvelope" / "Bundle0_2" / "Bundle0_3" to confirm. If the goldens reference bundle internals, they ARE affected.

**Refined position**: don't guess. Verify.

### Q11: Determinism risks

**Author's position in the audit**: ECDSA k-nonces, DSSE PAE serialization (UTF-8 vs byte length), Rekor serialization.

**Self-attack**:

The audit was right to flag these but didn't size them:

- **ECDSA k-nonces**: cosign Go has used RFC 6979 deterministic ECDSA since ~v2.0. Sigstore-rs's signing uses `ecdsa::signature::DigestSigner` which is RFC 6979 deterministic in the `p256` crate. So k-nonces aren't a determinism problem in either case. The audit overstated this risk.
- **PAE UTF-8 vs byte length**: this IS a real risk for Option X. The DSSE spec says PAE uses `len(payload_type)` and `len(payload)` — but "len" is ambiguous (Unicode codepoints? UTF-8 bytes?). In practice it's UTF-8 bytes (which matches `String::len()` in Rust) but a careless implementation could write Unicode codepoint count and produce non-interoperable PAE bytes. **Option X needs an explicit test asserting our PAE byte-encoder matches cosign Go's.** This is a real implementation hazard.
- **Rekor entry serialization**: Rekor v2's entry types are protobuf-JSON-serialized. Field order, default-field emission, lowerCamelCase rendering all matter for byte-identity. Sigstore-rs handles this correctly (uses protobuf-serde with the right config). Cosign Go also handles it correctly. Option V (cosign Go for sign) inherits cosign Go's behavior. Option X has to re-implement and the byte-identity gate is the safety net.
- **Integrated time + log index**: Rekor's `integratedTime` and `logIndex` are non-deterministic across runs. They're part of the bundle. The existing strip-set in `canonicalize_for_compare` handles this. Confirmed in `docs/diagrams/sprint-4/verify-flow.md`.

**Refined position**: the determinism risks are real but mostly bounded by existing infrastructure (k-nonces aren't a problem; strip-set handles Rekor timing fields). The dominant risk is PAE byte-encoding correctness, and Option V dodges it entirely.

---

## 6. New risks the audit didn't list

Things I noticed on this second pass that the original audit didn't include:

### R1: The empty-bytes-emailAddress fork patch may itself be wrong

The audit treated the fork's `60af47e` patch (empty bytes for absent email) as orthogonal. But cosign Go OMITS the emailAddress attribute entirely when None; our fork passes empty bytes. This is a behavior divergence that may or may not matter for Fulcio but is a latent inconsistency. If the fix path is X/Y, the fork's CSR construction should probably also be changed to omit the attribute entirely (matching cosign Go). The audit didn't loop back to this.

### R2: The `attest_sign` comment-vs-code drift is a smell

`sign.rs:99-101`'s comment says "Builds the DSSE envelope, signs with the ephemeral private key, submits the envelope + cert chain to Rekor v2" — describing behavior that doesn't exist. This was written by an earlier Claude Code session that misunderstood sigstore-rs's API. What other comments in attest_sign / attest_verify / related modules are wrong by the same logic? A grep pass through `crates/attestrum-attest/` for "DSSE" / "envelope" comments would surface them.

### R3: The Rekor entry type is currently `Hashedrekord`

Per `src/bundle/sign.rs:164` (sigstore-rs source), the Rekor entry submitted on sign is `ProposedLogEntry::Hashedrekord` — Rekor v2's entry type for blob signatures, not for DSSE attestations. So even if we somehow fixed the bundle to be DSSE-shaped on the bundle.json side, the Rekor entry would still be the wrong type. **Fix paths X/Y/V all need to ALSO change the Rekor entry type to `Dsse` or `Intoto`**. The audit mentioned this briefly under Option X cons but didn't make it a first-class consideration.

### R4: The current bundles in the wild are unverifiable by cosign

If the diagnosis is right, EVERY bundle Attestrum has ever signed (including any test runs, demo runs, etc.) is cosign-incompatible. There aren't real corpus bundles yet (we're pre-MVP), but if any demo bundles were shared with partners (AI2, Pleias, etc.) they need to be re-issued post-fix. The audit didn't surface this as a customer-comms / outreach implication.

### R5: The handoff document's 3 hypotheses were all written by an AI

The handoff doc at `~/.claude/plans/cosign-interop-verify-side-handoff-2026-05-25.md` proposed three hypotheses — all of which today's diagnosis says are wrong. The handoff doc itself was written by a Claude Code agent earlier today. **This is a pattern**: a structural bug was framed as three workload-identity-specific hypotheses because the prior agent was anchored on "we just fixed the workload-identity JWT bug; this must be related". The bias was wrong.

Reviewers should ask: what other planning docs in `~/.claude/plans/` are anchored on wrong premises that the next agent will inherit? Worth a sweep before Sprint 5's next planning cycle.

### R6: PATH-A-BRIEF.md's §1.5 promise may have been written without full sigstore-rs API verification

If sigstore-rs 0.14.0 truly has no DSSE sign API on the public path, then PATH-A-BRIEF §1.5's promise (cosign-verifiable bundles) was always going to require either Option X / Y / V. The fact that this wasn't caught until Sprint 4 E4.5's first CI run suggests the spec was written aspirationally without API verification. Reviewers should consider: what other PATH-A-BRIEF promises might have similar verification gaps? Cosign-interop for the proof predicates (inclusion / non-inclusion) at E11 is the obvious next surface to audit.

---

## 7. Refined recommendation

Updated from the audit's "Option X with eventual Y upstreaming":

**Phase 1 (now, ~1 day)**: Verify the diagnosis with a debug-instrumentation push. Add a `eprintln!` of the bundle's `content` variant + `mediaType` to the cosign_interop test, push, read CI log. Confirm bundle is `messageSignature` + `Bundle0_2`. This converts the 85% confidence to 99% and costs almost nothing.

**Phase 2 (after diagnosis confirmed, ~1-2 days)**: Implement **Option V** (cosign Go for sign, sigstore-rs for verify). Smallest change that restores cosign-interop. Sprint 5 E11 unblocked. Determinism risks bounded. Fork's JWT-parse patch can stay on the verify side; the sign side uses cosign Go natively.

**Phase 3 (deferred — Sprint 6 or post-MVP)**: Implement **Option Y** as a follow-up — upstream a DSSE sign API to sigstore-rs. Restores pure-Rust sign path. Cosign Go subprocess dependency goes away. Strong Path A acquirer narrative. Not urgent.

**Why not Option X first?**

The audit recommended X. On second pass, X's 4-8 day estimate + its byte-determinism implementation hazard + Sprint 5 timeline pressure all push toward V instead. X can be the v0.2 cleanup if the cosign Go subprocess feels wrong.

**Why not Option Z?**

Acquirer-hostile. PATH-A-BRIEF §1.5 + §12 are load-bearing for the Path A pitch. Z permanently breaks them. Author rejects.

**Why not Option W?**

W proposes cosign Go for BOTH sign and verify. V is W minus the verify side, which is a strict subset of complexity. If you'd take W you'd take V; if V works you don't need W.

---

## 8. Open questions the self-review surfaces

Things the founder should consider before picking the path:

1. **Is the cosign Go subprocess dependency acceptable?** Option V hinges on this. If "Attestrum is a single deterministic Rust binary" is a hard requirement, V is out and we're back to X/Y.

2. **Is upstream sigstore-rs PR latency acceptable?** If yes, Y becomes more attractive than V (better long-term fit). If no, V wins.

3. **Should the next planning cycle bundle in a wider sweep?** The audit author / self-reviewer noticed several latent issues (the comment-vs-code drift in attest_sign per R2; the Rekor entry type per R3; the wider comment-grep per R2). A "fix cosign-interop + sweep related anti-patterns" plan would be larger but more defensible.

4. **Should the founder loop in an outside Sigstore expert?** The Sigstore Slack / GitHub Discussions has maintainers who'd verify the "no DSSE sign API" claim in <5 minutes. If the author is wrong about that, the whole option matrix changes. Cheap insurance.

---

## 9. Confidence calibration

Now that the self-review is written, how confident am I in this self-review itself?

- **Diagnosis (§6 of audit + §2 here)**: 85% before Phase 1 verification; 99% after.
- **Option V is real**: 95% (subprocess invocation is well-trodden; cosign Go does support DSSE attestation signing per the CLI help).
- **Effort estimates**: ±50%. Software is hard to estimate.
- **Sprint 5 timing pressure**: 80% — depends on whether the founder treats E11 as a hard deadline.
- **R1-R6 are real risks**: 80% each (some are speculative).

The self-review's biggest residual risk is that the author is wrong about cosign Go supporting DSSE attestation signing via a simple `cosign sign-blob --attestation` invocation. If that subcommand doesn't exist or doesn't produce a Bundle v0.3 with DSSE envelope, Option V collapses. **Reviewers: please verify this before recommending V.**

---

*Self-review drafted 2026-05-25 by Claude Opus 4.7 (1M context). This file is intentionally a companion to the main audit doc; do not read this before reading the audit if you're an independent reviewer. The author considers this self-review honest but obviously biased.*
