# Attestrum cosign-interop — synthesis + decision

**Date**: 2026-05-25
**Author**: Claude Opus 4.7 (1M context), original audit author + self-reviewer
**Purpose**: parse the three review responses (two external + one self) against each other and formulate concrete next actions for the founder.
**Inputs** (all five docs committed together as the audit pack at repo root):
- `attestrum-cosign-interop-audit-2026-05-25.md` — original audit, 805 lines
- `attestrum-cosign-interop-audit-2026-05-25-self-review.md` — author's self-critique
- `attestrum-cosign-interop-audit-2026-05-25-reviewer-1-response.md` — **Reviewer 1**
- `attestrum-cosign-interop-audit-2026-05-25-reviewer-2-response.md` — **Reviewer 2** (Claude Opus 4.7, web/chat)
- `attestrum-cosign-interop-decision-2026-05-25.md` — this synthesis

---

## 1. Bottom line up front

**All three reviews converge.** The diagnosis holds (~95% confidence across the board), Options Z and W are rejected, the answer is some flavor of "build DSSE machinery ourselves, eventually upstream to sigstore-rs", and the work must land **before Sprint 5 S5-D1 E5 (API freeze)**.

**Author's self-review was wrong on Option V**. Both external reviewers explicitly considered and rejected subprocess-to-cosign-Go paths. The 5-day engineering savings doesn't justify the loss of the Rust-only narrative and the violation of BUILD-PLAN §6.2's implicit-subprocess anti-pattern. Withdrawing the V recommendation.

**Synthesized recommendation**: **Option X executed as Reviewer 2's "X → Y hybrid" with Reviewer 1's per-step rigor**. Build the DSSE construction logic as a cleanly-extractable module inside Attestrum first (tight iteration loop), then extract to a third commit on the existing sigstore-rs fork (Reviewer 1's `SigningSession::sign_dsse` API surface), then upstream the fork patch as a PR (Option Y proper). Each phase is independently shippable; the eventual upstream PR is a code-move, not a rewrite.

**Before any code lands, a ~1-2 hour verification cycle** (combining all three reviewers' verification asks into a single push) should close the residual 5-10% diagnostic uncertainty + resolve two pre-decision unknowns (Rekor entry type + fork patch site).

---

## 2. Where all three reviews agreed

The signal here is strong — three independent passes (one from the same author wearing a self-critic hat, two from outside reviewers) converge on the same conclusions:

| Topic | Consensus |
|---|---|
| Is §6 diagnosis correct? | **Yes**, all three at ~95% confidence. The signed-bytes/verified-bytes mismatch is the symptom of "sigstore-rs 0.14.0 missing the attestation-signing API". |
| Option Z (accept MessageSignature)? | **Reject**. Acquirer-hostile; breaks PATH-A-BRIEF §1.5 + §12 permanently. |
| Option W (full cosign-Go subprocess)? | **Reject**. Rust-only thesis matters; BUILD-PLAN §6.2 anti-pattern; weaker acquirer story. |
| Timing? | **Before S5-D1 E5**. Freezing the wrong bundle shape costs more than the schedule slip. |
| JWT-parse fork patch (`60af47e`)? | **Stays**. Real bug, orthogonal to the cosign-interop bug. |
| Rekor entry type? | **Critical implementation detail** — must be picked correctly before code lands. |
| Diagram-first? | **Yes**, sign-flow.md updates as commit 1 (CLAUDE.md §5 gate). |
| Determinism handling? | **Needs careful separation** between "unsigned corpus + manifest are byte-identical across targets" (still true) and "live signing events produce identical bundles" (never true, don't promise it). |
| The disabled `cosign/` module in sigstore-rs? | **Worth a 30-min grep but unlikely to save us**. Both Reviewer 1 and Reviewer 2 agree those files are OCI-layer / re-export only. |

---

## 3. Where the reviews diverged

The three reviews split on **where** the DSSE-signing code physically lives during the in-between period (between "shipped working in Attestrum" and "upstreamed to sigstore-rs"):

### Reviewer 1 — Modified Option Y (fork-first)
- Build `sign_dsse` directly on the Attestrum/sigstore-rs fork as a third commit.
- Attestrum's `attest_sign` calls the new fork API.
- Upstream the fork patch as a sigstore-rs PR.
- **Argument**: signing behavior belongs in the Sigstore SDK layer, not the Attestrum product crate. Cleaner architecturally from day one.
- **Cost**: every iteration goes through fork-push → Cargo.toml rev bump → cargo update → test. Tight iteration loop is slower.

### Reviewer 2 — Option X as X→Y hybrid (Attestrum-local first)
- Build DSSE machinery as a new module inside `attestrum-attest`, **designed for extraction**. Module's public surface uses sigstore-rs primitives, no Attestrum-shaped types.
- Ship from Attestrum-local code; iterate tight.
- After it works, extract the module to the fork as the third commit; offer upstream PR.
- **Argument**: upstream PR review on sigstore-rs is unbounded (multi-month plausible). Sprint 5 cannot block. Ship today, offer upstream in parallel, accept their timeline.
- **Cost**: one extra "extraction" step. Sligher architectural mismatch in the in-between period.

### Author self-review — Option V (cosign-Go for sign only) — **WITHDRAWN**
- Build the sign path as a subprocess call to `cosign sign-blob --attestation`.
- Keep verify in pure Rust.
- **Argument**: smallest change, fastest ship, avoids re-implementing DSSE PAE construction.
- **Rejected by both external reviewers** for three converging reasons:
  - Cosign Go bundles aren't byte-deterministic across runs (RFC 6979 k-nonces + Rekor timestamp embedding). Determinism is a CLAUDE.md §7 invariant.
  - Subprocess invocation is a BUILD-PLAN §6.2 anti-pattern.
  - Acquirer story is materially weaker.

**Author's withdrawal note**: Reviewer 1's framing ("do not shell out to cosign Go unless the Rust path proves blocked") and Reviewer 2's three independent rejection reasons both land. The author overweighted the engineering-velocity gain (~5 days) against the strategic loss (Rust-only narrative). Aligning with the consensus.

### Resolution: blend Reviewer 2's iteration discipline with Reviewer 1's final location

Reviewer 1 and Reviewer 2 aren't actually contradicting — they differ on the **ordering** of the same fundamental approach. The author's synthesis:

1. **Implementation phase**: Reviewer 2's path — build in `attestrum-attest` as an extractable module with the discipline that the public surface uses only sigstore-rs primitives (no Attestrum-shaped types). Tight iteration loop on Attestrum-local code.
2. **Stabilization phase**: extract the module to a third commit on the fork (`Attestrum/sigstore-rs`'s `attestrum/email-optional-for-workload-identity-tokens` branch, or a new branch if preferable). Verify the extracted code still passes Attestrum's tests when consumed back via `[patch.crates-io]`.
3. **Upstreaming phase**: file the fork-third-commit as a sigstore-rs upstream PR. Whatever the upstream timeline is, Attestrum is already shipped + working.

This captures Reviewer 1's architectural-correctness goal (`sign_dsse` ends up on `SigningSession` in sigstore-rs) and Reviewer 2's velocity goal (tight Attestrum-local iteration loop while developing).

---

## 4. New risks (consolidated from all three reviews)

Collecting the risks each reviewer raised. Some overlap; deduplicated below.

| # | Risk | Source | Decision-relevant before code? |
|---|------|--------|-----|
| **K1** | **Rekor v2 entry type choice** (`Intoto` vs `Dsse`). Cosign Go emits `Intoto`-flavored Rekor entries for in-toto attestations, not `Dsse`. Picking wrong sends us through another fix cycle. | Reviewer 1 (highest-risk implementation detail); Reviewer 2 C1; Self R3 | **YES** — verify against cosign Go source before coding. |
| **K2** | **Fork's `ade5422` empty-emailAddress patch site may be off the new code path** under Option X (which uses lower-level Fulcio CSR primitives, possibly bypassing `sign.rs:97-110`). Workload-identity OIDC bug could silently return. | Reviewer 2 C2 | **YES** — verify the patch site stays load-bearing, or fork-extend it. |
| **K3** | **DSSE PAE byte-determinism**. UTF-8 byte length vs character count; leading-zero / single-space pedantry in DSSE spec. Mistakes here silently emit cosign-incompatible bundles even AFTER "fix" lands. | Reviewer 1 (explicit); Reviewer 2 C4 | Mitigated by test-vector pinning at plan time. |
| **K4** | **The fork's empty-bytes-emailAddress patch itself may be a divergence from cosign Go** (cosign OMITS the attribute entirely; we pass empty bytes). Latent inconsistency. | Self R1 | Address as part of K2's investigation. |
| **K5** | **`attestrum-fingerprint::FingerprintBundle` may include bundle bytes in hash inputs**. Bundle shape change would cascade into fingerprint outputs. | Reviewer 2 C3 | 5-min grep at plan time. |
| **K6** | **`attest_sign` comment-vs-code drift sweep**. The current `sign.rs:99-101` comment describes DSSE behavior that doesn't exist. What other comments in attest_sign / attest_verify are wrong by the same logic? | Reviewer 1 Step 1; Self R2 | Address during diagram-first plan. |
| **K7** | **Diagram-first hard gate** for any sign/verify flow change. `docs/diagrams/sprint-4/sign-flow.md` (+ possibly `verify-flow.md`) must update before production code. Diagram-linter `--strict` mode blocks the commit otherwise. | Both reviewers | Commit 1 of the landing sequence is the diagram. |
| **K8** | **Sprint 5 + Sprint 6 entanglements**. Determinism goldens (S5-D1 E5), proof predicates (Sprint 5 E11), `attestrum prove` workflow, dataset card README, `verify.html` (Sprint 6) — all designed against the wrong bundle shape currently. | Reviewer 2 C6+C7; Self §7.4 | Plan a single re-issuance commit for any existing goldens. |
| **K9** | **Existing-bundles-are-known-bad**. Every bundle Attestrum has signed to date is MessageSignature-Bundle-v0.2 shape. Test runs, demo bundles, anything shared with partners (AI2/Pleias/etc.) needs re-issuance under new shape. | Self R4; Reviewer 2 C7 | Surface to founder; no partners yet so likely low blast radius. |
| **K10** | **Handoff doc had three hypotheses that were all wrong (anchor bias from prior session)**. Pattern worth checking: what other planning docs in `~/.claude/plans/` are anchored on wrong premises? | Self R5 | Sweep at next planning cycle if cheap. |
| **K11** | **PATH-A-BRIEF §1.5 promise was written without API verification**. What other PATH-A-BRIEF promises might have similar verification gaps (proof predicates E11 obvious next surface)? | Self R6 | Out of scope for this fix; flag for Sprint 5 plan-mode. |
| **K12** | **Determinism framing needs explicit separation** in the project narrative. Don't promise "byte-identical live signing events" — that's false for ANY Sigstore implementation. Promise "byte-identical unsigned corpus artifacts" + "deterministic bundle serialization for fixed signing inputs". | Reviewer 1 (explicit) | Documentation discipline, not a code change. |

**K1 and K2 are pre-code blockers.** Everything else is either mitigated by the plan-mode + diagram-first workflow already in place, or is a documentation/communication item the founder can choose to address inline.

---

## 5. Pre-implementation verification cycle (1-2 hours)

Before opening the plan-mode session for Option X, run a single verification push that closes the residual unknowns. This combines:
- **Self-review's Phase 1**: confirm bundle is `messageSignature` + `Bundle0_2` via debug-instrumented CI push.
- **Reviewer 2's C1**: verify cosign Go's preferred Rekor entry type for attestations.
- **Reviewer 2's C2**: verify fork patch site stays load-bearing under Option X's code path.
- **Reviewer 2's last paragraph + Reviewer 1's confirmation**: 30-min grep of `src/cosign/intoto.rs` + `src/cosign/signature_layers.rs` to fully close §10.1.3.
- **Reviewer 2's C3**: grep `attestrum-fingerprint` for bundle-byte dependencies.
- **Self R1 + K4**: confirm cosign Go's CSR-subject behavior for absent `email` claim.

### Verification checklist

```
[ ] V1. Re-trigger cosign-interop CI with a small diagnostic patch:
       - Add eprintln! of bundle.content variant + media_type to cosign_interop.rs
         BEFORE the verify call (so the bundle is materialized + visible even
         though verify panics)
       - Push, read CI log
       - Expect: "content: messageSignature" + "mediaType: ...bundle+json;version=0.2"
       - Action if not: revisit diagnosis from scratch
       - Time budget: 20-30 min (one push cycle)

[ ] V2. Read cosign Go's sign_blob.go for the Rekor entry type emitted for
       attestation signing:
       - Source: https://github.com/sigstore/cosign/blob/main/cmd/cosign/cli/sign/sign_blob.go
       - Or: cosign repo locally; grep "Intoto\|Dsse" alongside the attest-blob path
       - Expect: clear answer to "Intoto vs Dsse Rekor entry for cosign verify-blob-attestation"
       - Time budget: 15 min

[ ] V3. Read the disabled sigstore-rs cosign/intoto.rs + signature_layers.rs
       to fully close the §10.1.3 unknown:
       - Files: ~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/cosign/{intoto,signature_layers}.rs
       - Expect: confirm neither contains a public sign API for DSSE bundles
       - Action if it does: option matrix changes; surface to founder
       - Time budget: 30 min

[ ] V4. Trace how sigstore-rs's lower-level CSR construction would be invoked
       under Option X. Specifically: which function in sigstore-rs would Attestrum
       call to obtain a Fulcio cert without going through the high-level
       SigningSession::sign path?
       - Likely candidates: ctx.blocking_signer() returns the SigningSession;
         the session has access to the cert + private_key fields. If those are
         pub(crate), exposure requires a fork patch.
       - Expect: clear list of pub(crate) items that need fork-side exposure
       - Time budget: 30 min

[ ] V5. Confirm fork's empty-emailAddress patch site (sign.rs:97-110) stays
       load-bearing under the Option X code path. (If V4 reveals we'd use a
       different CSR-construction site, the patch needs to follow.)
       - Time budget: 15 min, bundled with V4

[ ] V6. Grep attestrum-fingerprint for any bundle-byte dependencies:
       rg -n "to_bundle|sigstore::bundle::Bundle|attest_sign" \
         crates/attestrum-fingerprint/src/
       - Expect: zero matches
       - Action if positive: surface to founder; bundle shape change cascades
         into fingerprint outputs
       - Time budget: 5 min

[ ] V7. Confirm cosign Go's CSR-subject behavior for absent email claim:
       - Source: https://github.com/sigstore/cosign/blob/main/cmd/cosign/cli/sign/sign_blob.go
         (search "subject" + "emailAddress")
       - Expect: cosign Go OMITS the attribute entirely; we currently pass
         empty bytes
       - Action: schedule a fork-patch update to match cosign Go behavior
         (either bundled with Option X's third commit or as a separate
         follow-up fork commit)
       - Time budget: 15 min
```

**Total verification budget**: ~2 hours of focused work, including one CI push cycle. Output: a written verification log added to this document at §10 or as a follow-up file.

**Decision gate**: V1 fails → diagnosis is wrong, halt and re-investigate. V3 surfaces a hidden sign API → option matrix changes. V6 positive → larger blast radius than scoped. All else green → proceed to plan-mode session for X→Y hybrid.

---

## 6. Plan-mode session sequence for the X→Y hybrid

After verification, the implementation work splits into 3-5 plan-mode sessions, each ending with founder approval before the next opens.

### Session 1 (1 hour planning + 4-8 hours execution): Diagram-first sign-flow update

Per CLAUDE.md §2 + §5, the diagram lands BEFORE production code. The session produces:

- Updated `docs/diagrams/sprint-4/sign-flow.md` (sequenceDiagram) showing the new DSSE-aware sign flow:
  - Manifest parquet bytes → SHA-256(manifest) → in-toto Statement subject digest population
  - In-toto Statement → canonical-JSON → DSSE envelope wrapping
  - PAE byte construction (with explicit byte-length semantics)
  - PAE signature via the ephemeral ECDSA-P256 key
  - Rekor v2 entry submission (entry type per V2's finding)
  - Bundle v0.3 protobuf-JSON assembly with `Content::DsseEnvelope`
- Frontmatter update: `source_of_truth: diagram` (during planning), bumped to `source_of_truth: code` after implementation
- `last_verified` SHA bumped per the 30-commit freshness rule

Commit 1 of the landing sequence is this diagram. Optional: an `attest_sign` comment-sweep commit that fixes the documented-but-not-implemented DSSE behavior comments BEFORE the diagram (Reviewer 1's Step 1).

### Session 2 (~2 days execution): Build the `dsse_sign` extractable module in Attestrum

Per Reviewer 2's "extractable module" discipline:

- New module `crates/attestrum-attest/src/dsse_sign.rs` with a single public function:
  ```rust
  pub fn sign_dsse(
      ctx: &SigningContext,
      id_token: &IdentityToken,
      payload_type: &str,
      payload: &[u8],
  ) -> Result<Bundle, AttestrumAttestError>
  ```
  Public surface uses sigstore-rs primitives only (no Attestrum-shaped types). This is the upstreamable-as-is API.
- Private helpers for: DSSE PAE encoding (with the K3 test vectors pinned), Bundle v0.3 protobuf-JSON assembly, Rekor v2 entry submission per V2's entry type pick.
- Unit tests for each step against DSSE spec test vectors (K3 risk mitigation).
- Updates to `attest_sign` (`crates/attestrum-attest/src/sign.rs`) to call `dsse_sign` with `payloadType = "application/vnd.in-toto+json"` and `payload = statement_payload` bytes.

### Session 3 (~0.5-1 day execution): Update `attest_verify` semantics (likely no changes)

Per Reviewer 1's Step 4 + Reviewer 2's diagnosis: the verify path is **conceptually correct**. sigstore-rs's DSSE verify path:
- Re-computes PAE from the bundle's `dsseEnvelope.payload` + `payloadType`.
- Verifies signature over PAE.
- Compares `bundle.dsseEnvelope.payload`'s embedded Statement.subject[0].digest.sha256 against `SHA-256(manifest_file)`.

So `attest_verify` keeps passing `manifest_path` as the input. The behavior change is entirely on the sign side. **No changes to `verify.rs` needed** beyond possibly removing or re-purposing the explicit `extract_in_toto_statement` (since sigstore-rs's verify now does the equivalent internally for DSSE bundles).

### Session 4 (~0.5 day execution): Tests + cosign-interop verification

- Add explicit assertions to `cosign_interop.rs`:
  - Bundle's `mediaType` is v0.3 (`application/vnd.dev.sigstore.bundle+json;version=0.3`)
  - Bundle's `content` variant is `dsseEnvelope`, not `messageSignature`
  - DSSE payloadType is `application/vnd.in-toto+json`
  - DSSE payload (base64-decoded) parses as the in-toto Statement
  - Statement subject digest matches SHA-256(manifest.parquet)
  - The shell-out to `cosign verify-blob-attestation --new-bundle-format` passes
- Add unit tests for `sign_dsse` covering: PAE byte-encoding correctness (via DSSE spec test vectors), Rekor entry shape, Bundle v0.3 protobuf-JSON correctness.
- Add a regression test against the OLD path: assert that no production code path in `attest_sign` calls sigstore-rs's `session.sign()` directly anymore (i.e., the MessageSignature path is fully decommissioned).

### Session 5 (~0.5 day execution): Extract dsse_sign to the fork as third commit

After Sessions 1-4 are committed + pushed + cosign-interop CI confirmed green:

- Open the fork repo at `https://github.com/Attestrum/sigstore-rs.git`, `attestrum/email-optional-for-workload-identity-tokens` branch.
- Cherry-pick / re-implement the `dsse_sign` module as a third commit (per Reviewer 1's API shape: `SigningSession::sign_dsse(payload_type, payload) -> SigstoreResult<SigningArtifact>`).
- Push the third commit.
- Bump Attestrum's `[patch.crates-io]` rev in `Cargo.toml`.
- Confirm cosign-interop CI stays green against the fork-side implementation.
- Update `docs/license-inventory.md`'s fork row.
- The Attestrum `dsse_sign` module is deleted or becomes a thin re-export shim during a transition window.

### Session 6 (out-of-band, multi-week): Upstream PR

After Session 5 stabilizes, submit the fork's third commit as a sigstore-rs upstream PR. This is asynchronous; Attestrum continues to work via `[patch.crates-io]` regardless of upstream timeline.

### Total effort estimate (corrected from self-review)

Sessions 1-5: **~5-8 days of focused work** end-to-end. Matches self-review's revised estimate of 4-8 days for Option X (the X→Y hybrid is X with a marginal additional Session 5 for the fork extraction, which is mostly a code-move).

Session 6 is multi-week but doesn't block Sprint 5.

---

## 7. Where the author was wrong (honest self-correction)

The author's self-review proposed **Option V** (cosign Go for sign, sigstore-rs for verify) as the smartest path. Both external reviewers explicitly rejected V-shaped paths:

- **Reviewer 1** (line 7): *"Do not shell out to cosign Go unless the Rust path proves blocked."*
- **Reviewer 2** (§B, on Option W which includes V): *"three independent reasons, any of which is sufficient"* (determinism, BUILD-PLAN §6.2 anti-pattern, weaker acquirer story).

**The author's error**: overweighted engineering velocity (~5-day savings vs Option X) against strategic narrative (Rust-only thesis, acquirer story, ecosystem contribution). For a 90-day MVP at a pre-acquisition stage, the narrative is worth more than the velocity. The author's self-review §4 said "Sprint 5 timing pressure favors Option V" but didn't price the long-term cost.

**Withdrawing** the V recommendation. Aligning with the consensus on X→Y hybrid.

---

## 8. What the founder needs to decide

Three explicit decisions before the next plan-mode session opens:

1. **Approve the X→Y hybrid as the chosen path?** Yes / No / Push back with refinement. The consensus is strong but the founder's call is decisive.

2. **Authorize the ~2-hour verification cycle in §5** as a pre-implementation gate? Yes / No. If yes, the next agent runs V1-V7 and reports back before opening plan-mode for Session 1.

3. **Filing the upstream sigstore-rs PR (Session 6) in parallel with Session 2's Attestrum-local development**, or sequentially after Session 5 ships? Reviewer 2 weakly prefers parallel ("the upstream review clock starts early"); Reviewer 1 implicitly sequential. Author's view: sequential is lower-risk (one moving target at a time) but adds maybe a week to the upstream timeline.

Two implicit decisions the founder may want to weigh:

4. **Should Sprint 5 E5 (API freeze) be paused until cosign-interop is green?** All three reviews say yes. Confirming.

5. **Should the bundle re-issuance plan (K9) require pinging any external recipients?** Author's understanding is there are no real partner bundles yet (pre-MVP, no AI2/Pleias outreach yet). Confirming.

---

## 9. Recommended single next action

Open a Bash session and run V1 (re-trigger cosign-interop CI with diagnostic patch). This is the highest-value 30-min investment available right now:

- Closes the 5-10% diagnosis uncertainty.
- Confirms what the actual emitted bundle looks like.
- Sets up the next plan-mode session with maximum confidence.
- Costs almost nothing.

If V1 confirms the diagnosis (~99% certain it will), proceed to V2-V7 in any order, then open Session 1 (sign-flow.md diagram update) for founder approval.

If V1 surprises us (bundle is actually DSSE-shaped, or some other anomaly), halt and re-diagnose. This is the cheap insurance the audit author should have proposed in the original audit but didn't.

---

## 10. Verification log (filled in as V1-V7 complete)

| Check | Status | Result | Notes |
|-------|--------|--------|-------|
| V1 — bundle content variant + mediaType | pending | — | Re-trigger CI after small diagnostic patch |
| V2 — cosign Go Rekor entry type for attestations | pending | — | Read sign_blob.go |
| V3 — cosign/intoto.rs + signature_layers.rs deep-read | pending | — | Close §10.1.3 unknown |
| V4 — sigstore-rs lower-level CSR primitives for Option X | pending | — | Identify pub(crate) items to fork-expose |
| V5 — fork patch site stays load-bearing under Option X | pending | — | Verify or fork-extend |
| V6 — attestrum-fingerprint bundle-byte deps grep | pending | — | Expect zero matches |
| V7 — cosign Go CSR-subject behavior for absent email | pending | — | Compare to our fork's empty-bytes approach |

---

## 11. Files referenced

| File | Purpose |
|------|---------|
| `attestrum-cosign-interop-audit-2026-05-25.md` | Original audit, 805 lines |
| `attestrum-cosign-interop-audit-2026-05-25-self-review.md` | Author's self-critique (V option withdrawn here) |
| `attestrum-cosign-interop-audit-2026-05-25-reviewer-1-response.md` | Reviewer 1's response (Modified Option Y) |
| `attestrum-cosign-interop-audit-2026-05-25-reviewer-2-response.md` | Reviewer 2's response (X→Y hybrid) |
| `/Users/austinmunday/.claude/plans/cosign-interop-verify-side-handoff-2026-05-25.md` | Original handoff doc (three hypotheses all wrong) |
| `/Users/austinmunday/.claude/plans/you-re-picking-up-stateless-marshmallow.md` | Today's Step 1 execution plan + status log |
| `/Users/austinmunday/Documents/Claude/attestrum/crates/attestrum-attest/src/sign.rs` | Where the bug lives + the misleading comment at line 99-101 |
| `/Users/austinmunday/Documents/Claude/attestrum/crates/attestrum-attest/src/verify.rs` | Step 1's instrumented file (today's commit `d3e352b`) |
| `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/ade5422/src/` | Cached sigstore-rs fork source (for V3, V4) |

---

*Synthesis document drafted 2026-05-25 by Claude Opus 4.7 (1M context), the original audit author. All three reviews converge on the X→Y hybrid path; verification cycle in §5 + plan-mode sequence in §6 + founder decisions in §8 + recommended single next action in §9 are the operative content. Author's Option V recommendation is explicitly withdrawn in §7.*
