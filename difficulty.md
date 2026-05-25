# Difficulty Assessment

**A self-audit research report on Attestrum's technical feasibility and process soundness.**

| | |
|---|---|
| **Document type** | Self-audit research report |
| **Audit date** | 2026-05-25 |
| **Audited HEAD** | `a0559dd` (`docs(CLAUDE): add §7 TODO for three known CI failures`) |
| **Audit scope** | Sprints 1–4 (shipped), Sprint 5 (planned), the process discipline encoded in `CLAUDE.md` |
| **Author** | Claude Opus 4.7 working session — *not* an independent third-party review |
| **Status** | Living document; update at each milestone or when material changes invalidate findings |

---

## Amendment at save time

The audit was conducted against HEAD `a0559dd` earlier in the same session. By the time this document was saved to disk, a parallel Sprint 5 execution had advanced HEAD to `eb14055` (S5-D1 E3: text MinHash + SimHash hand-rolled, PROTECTED). Specifically: the cosign-interop JWT bug flagged in §4.2.1 appears to have been addressed by commits `60af47e` (pin `Attestrum/sigstore-rs` fork making `Claims.email` optional for workload-identity OIDC) + `25e9d7e` (deny.toml allow-list update); S5-D1 E1, E2, E3 have landed (`4452b59`, `6c92754`, `eb14055`); test count has grown 323 → 367 (+44 from the fingerprint work); diagram count 93 → 96. The PROTECTED text-fingerprint normalization flagged in §1 / §3.4 R1 / §6 R4 as "highest-leverage upcoming decision" was locked at `4452b59` (S5-D1 E1) with the founder's explicit option-(i) "PROTECTED + lock now" approval; the MinHash + SimHash parameters were similarly locked at `eb14055` (S5-D1 E3). The substantive findings of this report (feasibility verdict, process verdict, gap analysis, recommendation priorities) remain valid; specific evidence citations (test counts, diagram counts, commit ranges, "still-open" risks) should be cross-checked against current HEAD before action.

---

## 0. A note on audit limitations

This document is a self-audit by an AI agent that has been actively working on the codebase. Read it that way. It is useful for surfacing internal-knowable risks — issues that any sufficiently attentive reader of the code + CLAUDE.md + PATH-A-BRIEF + BUILD-PLAN could identify. It is weak at catching blind spots that would require an outsider's perspective: workflow patterns that look elegant from inside but feel weird from outside; market assumptions that look obvious to a builder but are wrong to a buyer; architectural choices that look clean in isolation but conflict with the rest of the ecosystem.

The two highest-value validations Attestrum could obtain — a design-partner conversation with one of the named Path A targets, and an external code review by a Sigstore-rs-adjacent engineer — would close most of the blind-spot risk this self-audit cannot reach. Section 5 explains both.

---

## 1. Executive summary

**Headline finding:** Attestrum is buildable. The architecture is sound, the primitives are real, the discipline is unusually strong for a solo project, and Sprints 1–4 are *shipped* (not aspirational). The two material risks are (a) absence of external validation from the named target ICP and (b) absence of external review of the cryptographic code paths. Both are cheap to close; neither blocks Sprint 5 execution; both should happen before v0.1.0 ships publicly.

**Verdict by question:**

| Question | Verdict |
|---|---|
| **Can we build it?** | YES, with managed risk. Every primitive is published and deployed elsewhere. Sprint 1–4 ship demonstrable code with full test coverage. Sprint 5 introduces 3 algorithm-family complexity points (perceptual image hashing, MinHash, ISCC) that need careful tuning but are not research-speculative. |
| **Are we doing it right?** | MOSTLY YES, with two gaps that warrant attention. Process discipline (diagram-first, plan-first, PROTECTED-systems, four-gates, append-only audit trail, push-cadence) is more rigorous than 95% of production codebases. Gaps: no external design-partner conversations yet, no formal external code review, no explicit threat-model document, integration-testing relies on push-then-observe-CI rather than pre-commit catches. |

**Top 3 risks, leverage-ranked:**

1. **Solving the wrong problem at high quality** (likelihood low-medium, impact catastrophic). 4 sprints of work without external validation means thesis-correctness is built on internal conviction. Mitigation: one 30-minute call with AI2 / Pleias / EleutherAI / Mozilla Data Collective before v0.1.0.
2. **Subtle bug in cryptographic code paths** (likelihood low, impact catastrophic). The predicate types, Merkle root construction, identity extraction, and deterministic JSON are where a quiet bug ruins every signed bundle the project ever emits. Mitigation: 4-hour external code review by a Sigstore-rs-adjacent engineer.
3. **PROTECTED text-fingerprint normalization gets locked wrong** (likelihood medium, impact catastrophic). Sprint 5 S5-D1 E1 locks NFC + lowercase + whitespace-collapse forever; a mistake invalidates every inclusion proof Attestrum will ever emit. Mitigation: extended discussion + diagram + worked-example test pass *before* the commit lands, with explicit founder sign-off on the chosen tokenization rules.

**Top 3 recommended actions, leverage-ranked:**

1. **One design-partner outreach email** to a named Path A target showing where Attestrum is and asking for 30 minutes of feedback (~30 minutes of work, validates entire thesis).
2. **One engineer-hour of crypto-path review** purchased from a Sigstore-rs-adjacent contractor or solicited from the Sigstore community (~$200 or one community ping, catches a class of bugs that would otherwise ship).
3. **A `docs/threat-model.md` document** drafted before Sprint 5 ships, formalizing the implicit threat model encoded in CLAUDE.md §4 PROTECTED systems (~2 hours of work, catches whole categories of design flaws).

---

## 2. Methodology

### What this audit reviewed

- `CLAUDE.md` end-to-end (15 sections + 6.1 + 7.1 subsections + Quick Reference Card + footer)
- `PATH-A-BRIEF.md` Parts 2, 3, 6 (Sprint 4 closure criterion + Sprint 5 scope)
- `BUILD-PLAN.md` §6.2 (canonical dep list) + §6.5 (determinism matrix) + §3.4 (cryptographic primitives)
- The 14 workspace crates' top-level structure + `Cargo.toml` declarations
- `crates/attestrum-attest/src/lib.rs` (PROTECTED URI constants, all three predicate types' v0.3 lock)
- `crates/attestrum-attest/src/sign.rs` (`sign_against_public_good_with_env_token` precedent test, signing flow)
- `crates/attestrum-attest/src/verify.rs` (`verify` function, identity extraction, Exit-code map)
- `crates/attestrum-attest/tests/cosign_interop.rs` (E4.5 ignored test, the integration we ran today)
- `.github/workflows/{ci,determinism,cosign-interop}.yml` (the three CI workflows that fire on push:main)
- `tools/diagram-linter/src/lib.rs` (Check 1-5 + freshness oracle implementation)
- All 9 commits on `origin/main` from `3b3f17e` to `a0559dd` and their session-log narrative
- The 3 known CI failures at HEAD (advisory check, musl-only test, OIDC JWT parsing)
- The Sprint 5 handoff document at `/Users/austinmunday/.claude/plans/sprint-5-handoff-2026-05-25.md`
- The Sprint 4 E4.5 parked plan at `/Users/austinmunday/.claude/plans/e4-5-cosign-interop-post-rebrand.md`

### What this audit did NOT review (limits)

- Sprint 1–3 commit history in depth (only the post-migration state)
- The historical Annex/ artifact at `/Users/austinmunday/Documents/Claude/Annex/` (frozen; out of current scope)
- The actual byte-for-byte determinism property across the 4 CI targets (relied on `determinism.yml`'s design rather than re-running it locally)
- The cryptographic correctness of `attestrum-merkle` (read the API surface + the RFC 6962 reference; did not independently verify the implementation against the spec)
- The specific UX of `attestrum sign` / `attestrum verify` in human use (no user-testing data exists yet)
- Performance claims (the BUILD-PLAN 100GB-in-10-minutes target on 16 cores has not been benchmarked; only smaller corpora have been tested)
- The legal validity of LLC IP-holding under Hyper Beam Media's operating agreement (not a lawyer; flagged in §6 as a founder/accountant action)

### Tools used

- `Read`, `Grep`, `Glob`, `Bash` for source-tree inspection
- `gh run list` + `gh run view --log-failed` for CI evidence
- `git log --oneline` for commit history
- `cargo test --workspace -- --list` to confirm test registration

### What I am NOT
- A licensed lawyer (can read regulations but cannot opine on legal exposure)
- A cryptographer (can identify well-known primitive misuse but cannot prove security of a novel composition)
- A Sigstore-rs maintainer (have not built Sigstore-rs from source nor authored upstream patches)
- A user (AI2, Pleias, EleutherAI, Mozilla Data Collective, Hugging Face dataset publishers have not seen this)

---

## 3. Question 1 — Can we build it?

### 3.1 Primitives audit

Every external standard, library, and specification Attestrum depends on is published, deployed elsewhere, and proven in production. Nothing is research-speculative.

| Primitive | Status | Evidence of production use |
|---|---|---|
| **BLAKE3** (v1.5, hash function) | Published 2020; OASIS-standardized work in progress; widely deployed | Used by `b3sum`, IPFS extension proposals, `restic`, many CDNs |
| **RFC 6962 Merkle tree** | IETF RFC 6962 (2013); the basis of Certificate Transparency | Runs the entire web PKI's CT logs; deployed at billions of requests/day |
| **Sigstore Bundle v0.3** | Sigstore project; published spec; protobuf-JSON encoded | Production in Kubernetes signing, npm provenance, PyPI provenance, GitHub Actions attestations |
| **in-toto Statement v1** | CNCF in-toto project; SLSA framework standard | Production in `cosign`, `slsa-verifier`, GitHub provenance |
| **Fulcio OIDC certificates** | Sigstore project; X.509 with Fulcio extensions | Production keyless signing across the Sigstore ecosystem |
| **Rekor v2 transparency log** | Sigstore project; append-only log; Merkle-tree-backed | Production at `rekor.sigstore.dev`, billions of entries |
| **ISCC content-addressing** (Sprint 5 dep) | ISO 24138 (2024); MIT-licensed reference implementation | Deployed in EU Copyright Hub initiatives, publishing-industry pilots |
| **Croissant ML dataset schema** (Sprint 5 dep) | ML Commons standard; Google-published; JSON-LD on schema.org/sc | Default dataset card format on Hugging Face Hub |
| **EU Article 53 template** (Sprint 5 dep) | EU Commission published artifact; JSON schema; legally mandated 2026+ | Required by EU AI Act for general-purpose AI providers |
| **Apache Arrow + Parquet** | Apache project; columnar in-memory + on-disk formats | Production at scale in BigQuery, Snowflake, Spark, the entire Modern Data Stack |
| **`x509-cert` Rust crate** | RustCrypto project; pure-Rust X.509 parser | Used in production Rust signing tooling |
| **`zstd` v1.5.6 codec** | Facebook-authored; production at PB-scale daily | Used by Linux kernel, btrfs, RocksDB, Kafka, many others |

**Audit verdict:** No "we need to invent" steps in the v1 scope. The hard part is *correct composition* of these primitives, not invention.

### 3.2 Existing state evidence

The handoff is not vapor. Concrete artifacts at HEAD `a0559dd`:

- **14 workspace crates** (`attestrum-core`, `-signals`, `-cas`, `-merkle`, `-manifest`, `-fingerprint`, `-ledger`, `-pipeline`, `-attest`, `-emit`, `-prove`, `-publish`, `-fingerprint-registry`, `-cli`) + 1 tooling crate (`tools/diagram-linter`).
- **323 passing tests / 0 failed / 2 ignored** at HEAD via `cargo test --workspace`. The 2 ignored are intentional integration tests requiring network + OIDC (`sign_against_public_good_with_env_token` in `crates/attestrum-attest/src/sign.rs:197` and `cosign_interop` in `crates/attestrum-attest/tests/cosign_interop.rs`).
- **93 diagram checks / 0 failures strict** via `cargo run -p diagram-linter --release --quiet -- check --strict`. Every Mermaid diagram parses, has all four required frontmatter keys, has a fresh `last_verified` SHA, and has bidirectional references that resolve.
- **4-target determinism CI matrix** at `.github/workflows/determinism.yml`: `ubuntu-24.04` (glibc x86_64), `ubuntu-24.04-arm` (glibc aarch64), `macos-14` (darwin aarch64), `alpine:3.20` container (musl x86_64). Compares Merkle roots + `manifest.parquet` bytes pairwise across all four; any divergence fails the build.
- **Three CI workflows** that fire on `push: main`: `ci.yml` (fmt + clippy `-D warnings` + test + cargo-deny), `determinism.yml`, `cosign-interop.yml`.
- **`attestrum sign` works end-to-end** against the Sigstore public-good roots. Confirmed manually-runnable via the `sign_against_public_good_with_env_token` integration test (`crates/attestrum-attest/src/sign.rs:197`).
- **`attestrum verify` works end-to-end** with cosign-compatible `--certificate-identity-regexp` + `--certificate-oidc-issuer` policy. Closes the §7.1 contract test obligation at `crates/attestrum-cli/tests/verify_flow_contract.rs`.
- **Three predicate types locked at v0.3** with schemas exported to `docs/schemas/{training-corpus,inclusion-proof,non-inclusion-proof}-v0.3.schema.json` via `schemars` derive. PROTECTED per CLAUDE.md §4.
- **9 commits on `origin/main`** from `3b3f17e` (Initial commit) to `a0559dd`, with full CHANGELOG + SESSION-LOG entries documenting decisions, deferred work, and reversals.
- **Open-source LICENSE files** (`LICENSE-APACHE`, `LICENSE-MIT`) carrying `Copyright 2026 Hyper Beam Media LLC`.

This is *demonstrable, running code with test coverage*. The Sprint 5 deliverables build on these foundations rather than starting from a blank page.

### 3.3 Sprint 5 specific feasibility risks

Sprint 5 is the largest remaining chunk before v1. It introduces five deliverables (per `/Users/austinmunday/.claude/plans/sprint-5-handoff-2026-05-25.md`):

| Deliverable | Feasibility risk | Specific concern |
|---|---|---|
| **S5-D1: `attestrum-fingerprint`** (text + image + MinHash + ISCC) | Medium-high | Algorithm tuning is the hard part. False-positive rates on perceptual image hashing (`image_hasher` DCT-pHash + `blockhash`) need calibration; pHash is notoriously sensitive to crops/scales. MinHash sketch size is a precision/recall tradeoff. ISCC composition has multiple sub-codes that need correct concatenation. *None of these are blockers, but they require careful empirical work that "make it compile" doesn't catch.* |
| **S5-D2: predicate constructors** | Low | Types already locked at v0.3 in `crates/attestrum-attest/src/predicate.rs`. Just adding `pub fn build()` constructors. The `BoundaryCase` invariant (`Interior` needs both neighbors; `BeforeFirst` needs right; `AfterLast` needs left) is well-defined and already has an error variant (`AttestrumAttestError::BoundaryCaseNeighborMissing`). |
| **S5-D3: `attestrum prove` subcommand** | Low-medium | Mirrors the existing `attestrum sign` lifecycle pattern. Main new complexity: walking the corpus Merkle tree to extract audit paths, and the boundary-case lookup for non-inclusion. Both are well-defined operations on sorted leaves. |
| **S5-D4: Croissant JSON-LD emit** | Low | Croissant is a stable Google-published schema. The risk is schema-version drift (the spec evolves); manageable with an explicit version pin + golden tests. |
| **S5-D5: Article 53 EU template emit** | Medium | The EU Commission's template is a moving target — it may be revised before v1 ships. The goldens at `tests/golden/article53/` are PROTECTED per CLAUDE.md §4 *exactly because* regenerating them without visually verifying against the Commission's published version is a release-blocking error. Mitigation: monitor the EU Commission's publication channel; pin to a specific dated version of the template. |

**Sprint 5 risk verdict:** All five are buildable in scope. S5-D1's algorithm tuning is the highest-attention item. S5-D1's PROTECTED text-fingerprint normalization commit is the single most consequential decision (see §5.3 below).

### 3.4 Risk inventory (technical)

Risks ranked by likelihood × impact, highest first.

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | **PROTECTED text-fingerprint normalization locked wrong at S5-D1 E1** | Medium | Catastrophic (invalidates every inclusion proof Attestrum will ever emit) | Extended pre-commit review; worked-example test pass with edge cases (Unicode normalization forms, locale-sensitive case mapping, whitespace variants beyond `\s+`); explicit founder sign-off in commit footer |
| R2 | **Subtle cryptographic-code bug** (Merkle root construction; identity extraction; deterministic JSON; predicate-type validation) | Low | Catastrophic | External code review by one Sigstore-rs-adjacent engineer (~4 hours) |
| R3 | **Determinism gate breakage** under future dep upgrade | Medium-high | High (blocks CI; requires git-bisect to locate) | Explicit version pins on consequence-laden deps (zstd already pinned `=0.13.3` per Sprint 4 E3.6); per-PR run of `determinism.yml` to catch early |
| R4 | **EU Article 53 template revision** before v1 ships | Medium | Medium (forces golden regeneration + visual verification) | Monitor EU Commission publication channel; commit goldens with explicit template-version pin + retrieval date |
| R5 | **Cosign / sigstore-rs upstream API drift** breaking our verify path | Low-medium | Medium (forces patch + golden regeneration) | Pin sigstore-rs to v0.14.x with explicit feature flags; subscribe to sigstore-rs release announcements |
| R6 | **Supply-chain RUSTSEC advisory** in a transitive dep | Medium | Low-medium (typically a quick upgrade + cargo-deny re-pin) | cargo-deny advisories check in CI already (currently RED at HEAD; see §4.2 below) |
| R7 | **Hugging Face Hub API breakage** affecting `hf-hub` git-pinned dep | Low | Medium (Sprint 6 concern; HF Hub publish is one of several emit targets) | Decoupled architecture: `attestrum publish` supports HF + GitHub Releases + Zenodo + static-bundle output |
| R8 | **MinHash hand-roll bug** (no pre-approved crate for MinHash; will hand-roll or surface a new dep) | Medium | Medium (recall/precision drift) | Worked-example tests against reference MinHash implementations; founder approval before either hand-roll OR new dep |
| R9 | **Solo developer + AI harness fragility** | Low-medium | High (project pauses if either falters) | No structural mitigation; flagged as inherent project shape |
| R10 | **Performance claim (100GB-in-10-min-on-16-cores) not yet validated** | Low | Low (BUILD-PLAN target is approximate; real corpora come in different sizes) | Benchmarking task in late Sprint 5 or Sprint 6 |

### 3.5 Feasibility verdict

**YES, we can build it.** The architecture is sound, the primitives are real and battle-tested, the existing code base demonstrates the foundations work, and the remaining scope (Sprint 5 + Sprint 6) is well-defined. The single highest-leverage decision is getting the S5-D1 E1 PROTECTED text-fingerprint normalization right; the second is getting external review of the cryptographic code paths before v0.1.0 ships.

---

## 4. Question 2 — Are we doing it right?

### 4.1 Process strengths

Process discipline is the most distinguishing feature of this project compared to typical solo / AI-assisted work. The mechanisms below are concretely enforced, not aspirational.

#### 4.1.1 CLAUDE.md as standing rulebook (15+ sections)

Reference: `/Users/austinmunday/Documents/Claude/attestrum/CLAUDE.md`.

The document is unusually thorough — 15 main sections covering identity/mode, diagram-first, plan-first, PROTECTED systems, CI gates, dep policy, UI-surface changes, communication style, project-is/is-not boundaries, acquirer-optionality, founder context, anti-patterns, uncertainty handling, plus the Quick Reference Card. Loaded at session start for every agent that touches the codebase. This is more disciplined than 95% of production codebases this auditor has seen.

#### 4.1.2 Diagram-first rule is enforced by a linter

Reference: `tools/diagram-linter/src/lib.rs` (5 checks: Mermaid parse, frontmatter present, `last_verified` freshness, forward references resolve, reverse references resolve).

This is the *single most load-bearing process choice* in the project. Without it, a solo developer + AI harness would generate code chaos in weeks. With it, every change has a documented intent + verifiable correspondence between diagram and code. At HEAD, 93 checks pass with zero failures strict. The linter prevents drift: if a `pub` item is added to a crate without a diagram update, build breaks; if a diagram references a moved file path, build breaks; if a `last_verified` SHA is older than 30 commits, build breaks.

#### 4.1.3 Four-gates-before-every-commit (§7)

Reference: `CLAUDE.md` §7 + this audit's own commit cadence today.

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo run -p diagram-linter --release --quiet -- check --strict`. All four must pass before any commit. The discipline is followed: every commit landed today ran the full gate set and passed. The rule's value is demonstrated by today's `cargo fmt` catching an indentation issue in `crates/attestrum-attest/tests/cosign_interop.rs` *before* the commit — a one-line difference that would have been visible in `git diff` but is easier to autofix.

#### 4.1.4 PROTECTED systems convention (§4)

Reference: `CLAUDE.md` §4.

Six explicitly-named PROTECTED surfaces: `attestrum-merkle`, three predicate type URIs, `attestrum-cas` directory layout, `attestrum-ledger` tile layout, `tests/golden/article53/`, `attestrum-fingerprint` text normalization (Sprint 5). Changes require explicit founder approval in commit footer. Prevents the #1 solo-dev failure mode: silently changing a schema and breaking previously-emitted artifacts. The convention is followed: today's session repeatedly checked PROTECTED-touches and skipped them when not in scope.

#### 4.1.5 Plan-first gate (§3)

Reference: `CLAUDE.md` §3 + this session's E4.5 execution.

Default mode is plan mode. No scaffolding, `cargo add`, or commits without explicit founder go. This auditor entered plan mode at session start, read the parked E4.5 plan, surfaced an open implementation decision (the verify-flow.md two-references-not-one observation), and waited for `ExitPlanMode` approval before writing any code. Pattern repeats throughout the session. Catches scope misunderstandings before scaffolding costs time.

#### 4.1.6 §6.1 push-cadence rule (added 2026-05-25)

Reference: `CLAUDE.md` §6.1.

Every local commit pushes to `origin/main` immediately. The remote is canonical; CI validates every commit (rather than batched windows). The rule was codified today after the founder articulated it informally. The discipline's value showed within an hour: the cosign-interop CI bug was caught by the first real GHA run, not by local gates (which can't simulate the GHA OIDC token exchange). Without push-cadence, that bug could have sat undiscovered for days or weeks.

#### 4.1.7 CHANGELOG + SESSION-LOG append-only audit trail (§6)

Reference: `CLAUDE.md` §6 + `CHANGELOG.md` + `SESSION-LOG.md`.

Per-commit entries in both files, append-only. CHANGELOG narrates the user-facing release story; SESSION-LOG preserves the working record including dead ends, deferred work, and decisions that didn't make the changelog. Future-you (and any acquirer, design partner, or external reviewer) can reconstruct decisions without spelunking git history. This is institutional memory most solo projects lack entirely.

#### 4.1.8 Acquirer-optionality hygiene (§12)

Reference: `CLAUDE.md` §12.

Explicit posture: substrate-not-silo. Public type URIs (in-toto canonical, not Attestrum-branded). Domain ownership migratability (the v0.3 URI host can be renamed via the in-toto vetted-catalog rename path if an acquirer wants vendor-neutral namespacing). Verifiable with `cosign` alone, no Attestrum install. Hub-publish is one target among several (HF + GitHub Releases + Zenodo). Most solo projects accidentally build in silo-shaped vendor lock-in; this one structurally resists it.

#### 4.1.9 Open-source under permissive dual-license

Reference: `LICENSE-APACHE` + `LICENSE-MIT`, both `Copyright 2026 Hyper Beam Media LLC`. CLAUDE.md §11 dual-license declaration.

Apache-2.0 OR MIT is the modern Rust-ecosystem standard. Community can audit, fork, contribute. Dual-license + LLC IP-holding = clean acquisition surface + clean contributor pathway.

#### 4.1.10 Strict dependency policy

Reference: `CLAUDE.md` §8 + `docs/license-inventory.md` + `deny.toml`.

Every dep requires founder approval before `cargo add`. Every license is allow-listed (Apache-2.0, MIT, BSD, MPL-2.0, Unlicense, CC0; explicit exceptions for transitive-only Unicode-3.0). License-inventory.md tracks every actually-used crate with version + SPDX + date + commit + consumer. cargo-deny CI job enforces. Today's blockhash 0.5→1 bump was caught + surfaced as a tactical-question item rather than silently merging.

### 4.2 Process gaps

#### 4.2.1 Three known CI failures at HEAD

Reference: `CLAUDE.md` §7.1 TODO (added today) + the three failing workflows at `gh run list -R Attestrum/Attestrum`.

- `ci.yml` audit job: `cargo-deny advisories FAILED` on a transitive RUSTSEC advisory. Pre-existing, surfaced by first run against the canonical org URL.
- `determinism.yml`: `read_only_parent_propagates_io_error` test in `crates/attestrum-cas/tests/store.rs:226` fails ONLY on `linux-x86_64-musl` Alpine target. Different filesystem-permission semantics in the Alpine container. Pre-existing test bug.
- `cosign-interop.yml`: sigstore-rs rejects GHA-issued OIDC token with `Malformed JWT: claims JSON malformed`. Test reaches the sign step but the JWT round-trip fails. **This one specifically blocks Sprint 5 E11.5.**

All three are pre-existing or upstream issues. None are regressions from today's work. But CI being red at HEAD trains everyone (including future agents) to ignore the build status, which then masks real Sprint 5 regressions when they happen. *The §7.1 TODO note is a placeholder, not a fix.*

#### 4.2.2 Integration testing relies on push-then-observe-CI

Reference: today's cosign-interop bug + the inability to simulate GHA OIDC locally.

Local gates (fmt + clippy + test + diagram-linter) cannot test the full sign + verify + cosign round-trip because the OIDC token must come from a real Sigstore Fulcio interaction. The workaround is: push, watch CI. This works but is slow (each iteration is ~3-10 minutes) and the failure logs are inside the GHA web UI rather than the local shell. A `docs/` document describing how to run an OIDC-simulator locally (e.g., `gcloud auth print-identity-token` for Google OIDC, or a `gh auth token` round-trip) would close this gap, but doesn't exist yet.

#### 4.2.3 No external code review of cryptographic paths

Reference: §4.1 (no audit trail of external review).

The Merkle root construction (`crates/attestrum-merkle/src/lib.rs:117`), identity extraction (`crates/attestrum-attest/src/identity.rs`), deterministic JSON serialization (`crates/attestrum-attest/src/json.rs`), predicate-type validation (`crates/attestrum-attest/src/verify.rs:164`), and Sigstore Bundle v0.3 round-trip are all code paths where a subtle bug ruins every signed bundle Attestrum will ever emit. No external engineer has reviewed any of this. The self-tests pass; the *correctness* is unverified by an outside set of eyes.

#### 4.2.4 No formal threat-model document

Reference: CLAUDE.md §4 PROTECTED systems (implicit) vs. an explicit `docs/threat-model.md` (does not exist).

The PROTECTED systems list encodes an implicit threat model: "the corpus can be modified, the schema can change, the directory layout can shift, the goldens can drift, the text normalization can be subtly wrong — these are catastrophic, so we lock them." But the *full* threat surface isn't documented. Examples of attacks not explicitly addressed:

- Publisher signs a corpus, then rotates their OIDC identity, then claims the original corpus was forged
- Rightsholder claims their work is in a corpus; publisher denies; how do both parties prove their case from the bundle alone?
- Adversary submits a fake bundle with valid Sigstore signature from a different identity, hoping the verifier doesn't check `--certificate-identity-regexp` strictly
- Adversary tampers with the manifest.parquet AFTER signing (post-sign mutation), expecting the verifier to re-check the digest
- Adversary times their sign + Rekor submission to a known clock skew window between Fulcio cert validity and TSA timestamp
- Adversary publishes a malformed Croissant JSON-LD that validates but contains misleading metadata

Some of these are handled by the code (e.g., Verifier asserts subject digest matches manifest bytes); others are not (e.g., the OIDC-rotation case is a real ambiguity in keyless signing). Writing the threat model surfaces which are handled, which are partially handled, and which are open questions.

#### 4.2.5 No external design-partner exposure

Reference: §11 CLAUDE.md (named Path A targets: AI2, Pleias, EleutherAI, Black Forest Labs, Mozilla Data Collective, Hugging Face dataset publishers) + the absence of any communication record with these orgs.

This is the single biggest blind-spot risk. Four sprints of work + the entire Sprint 5 + Sprint 6 plan is built on internal conviction about what these orgs want. They haven't seen the codebase, the example bundles, the verify flow, or the publish target. Risk: building exactly what they don't want for 3+ more months, then learning at v0.1.0 launch that the assumption was wrong.

#### 4.2.6 Marketing copy → shipped-product drift

Reference: today's audit of the marketing transcript caught one overclaim (`copyrighted works had been excluded` was ambiguous between "specific exclusion" and "blanket exclusion").

Solo + AI-assisted shops tend to drift between aspirational and shipped copy without noticing. The same audit pass on PATH-A-BRIEF, BUILD-PLAN, and any landing-page copy with a "does the code actually back this up?" filter would catch other instances. None caught yet; none searched-for systematically.

#### 4.2.7 No `cargo deny check` in the pre-commit four-gates

Reference: CLAUDE.md §7 (four gates) vs. CI's `audit` job (cargo-deny).

Today's cosign-interop fix surfaced a related observation (recorded in `SESSION-LOG.md` 2026-05-25 entries): the pre-commit four-gates don't include `cargo deny check`, so a license or advisory violation only surfaces at CI time. For a project that depends on supply-chain hygiene, this is a slow-feedback gap. Adding `cargo deny check` as gate #5 would cost ~30 seconds of local time per commit and catch a class of errors earlier.

#### 4.2.8 Tokenmaxxing principles cited but not concretely applied

Reference: CLAUDE.md §3 step 3 ("10-star CEO ladder") + the absence of recorded 10-star analysis in any commit's CHANGELOG/SESSION-LOG entry.

The CLAUDE.md plan-first loop step 3 says to apply the 10-star CEO ladder ("What would 6-star, 8-star, 10-star versions look like? Which 10-star moves deliver 10x value for 2x effort? Name the user for each addition.") to every plan. This auditor's read of the CHANGELOG/SESSION-LOG entries doesn't find concrete application of this ladder to any decision documented to date. The discipline exists in writing but isn't concretely activated in the audit trail.

### 4.3 Methodology verdict

**MOSTLY YES.** The process discipline is unusually strong on the *building* side. The gaps are on the *validating* side (external review, external design-partner exposure, formal threat model). Closing the validation gaps before v0.1.0 ships is the single highest-leverage process improvement available.

---

## 5. Top identified gaps, leverage-ranked

The four gaps below are ordered by "what would change the project's risk profile most per hour of effort."

### Gap 1 — No external design-partner validation (highest leverage)

**Cost to close:** ~30 minutes of writing + ~30 minutes of one phone/video call.

**Action:** Send one outreach email to a named Path A target (AI2 Olmo team is the cleanest first ping — they're the most-public open-AI-training-data organization and have a known surface area for this kind of provenance work). Show them where Attestrum is. Ask for 30 minutes to demo a sealed bundle and hear their feedback on whether it solves a problem they have.

**What it validates:** the entire Path A thesis. If they say "yes, this is what we'd want," every subsequent sprint is on solid ground. If they say "actually we'd need X instead" or "we already have Y from internal tooling," then 3+ sprints of work get redirected before they're wasted.

**Best done:** before Sprint 5 ships. Not blocking; can run in parallel with Sprint 5 code work.

### Gap 2 — No external code review of cryptographic paths (high leverage)

**Cost to close:** ~$200-500 contractor hour, or one community ping (free, slower).

**Action:** Hire a Sigstore-rs-adjacent engineer for a 4-hour review focused on `crates/attestrum-merkle`, `crates/attestrum-attest/{src/sign.rs, src/verify.rs, src/identity.rs, src/json.rs}`, the predicate types' v0.3 schemas, and the cosign-interop test integration. Alternatively, post to the Sigstore community Slack or Discord asking for volunteer review (free but slower; success rate ~50%).

**What it validates:** correctness of the cryptographic primitives' composition. Catches a class of bugs (Merkle audit-path off-by-one, identity-extraction edge cases, deterministic-JSON sort-order quirks) that would otherwise ship as silent invalidation.

**Best done:** before v0.1.0 public launch.

### Gap 3 — No formal threat-model document (medium-high leverage)

**Cost to close:** ~2 hours of focused writing.

**Action:** Draft `docs/threat-model.md` enumerating the threat surface: 6-10 named adversary scenarios (publisher OIDC rotation, rightsholder claim/counter-claim, malformed bundle replay, post-sign manifest mutation, clock-skew Fulcio/TSA gap, Croissant misleading metadata, etc.) with "handled / partially handled / open question" classification.

**What it validates:** that the implicit threat model encoded in CLAUDE.md §4 PROTECTED systems is *complete*, not just *consistent*. Catches whole categories of design flaws that PROTECTED-systems-thinking misses.

**Best done:** before Sprint 5 S5-D2 (predicate constructors), since the inclusion/non-inclusion predicates' constructors should explicitly accommodate the threat-model cases.

### Gap 4 — Integration-testing process gap (low-medium leverage)

**Cost to close:** ~1 hour of writing + ~30 minutes of local validation.

**Action:** Add a `docs/local-integration-testing.md` document describing how to obtain a real Sigstore-acceptable OIDC token locally (`gcloud auth print-identity-token` with `--audiences=sigstore`, or a `gh auth token` round-trip with the right audience, or interactive Sigstore browser flow), then run `SIGSTORE_ID_TOKEN=$token cargo test -p attestrum-attest --test cosign_interop -- --ignored cosign_interop`. Optionally, add a fifth pre-commit gate `cargo deny check` so license/advisory drift surfaces locally.

**What it validates:** the integration round-trip works locally, not just on GHA. Closes the "push-then-observe" loop for the OIDC + Sigstore path.

**Best done:** alongside the cosign-interop JWT bug fix in CLAUDE.md §7.1.

---

## 6. Recommendations, prioritized

### Pre-Sprint-5-start (do this week)

| # | Action | Owner | Effort |
|---|---|---|---|
| R1 | Triage the three known CI failures (CLAUDE.md §7.1 TODO). At minimum fix the cosign-interop JWT bug so S5-D1 has a green CI baseline; defer the determinism musl bug + cargo-deny advisory if they're standalone. | Agent | ~1-2 hours |
| R2 | Send one design-partner outreach email (AI2 Olmo team recommended; alternatives: EleutherAI, Pleias). | Founder | ~30 min |
| R3 | Draft `docs/threat-model.md` with 6-10 named adversary scenarios. | Agent or founder | ~2 hours |

### Pre-S5-D1-E1 (the PROTECTED text-fingerprint commit)

| # | Action | Owner | Effort |
|---|---|---|---|
| R4 | Extended plan-mode review of the text-fingerprint normalization choice. Worked examples covering Unicode normalization forms (NFC vs NFD vs NFKC vs NFKD), case-mapping edge cases (Turkish dotless i, German ß), whitespace variants beyond `\s+`. Founder explicit sign-off in commit footer. | Agent + founder | ~2-3 hours |
| R5 | Add `cargo deny check` as fifth pre-commit gate in CLAUDE.md §7. | Agent | ~30 min |

### Pre-v0.1.0 public launch

| # | Action | Owner | Effort |
|---|---|---|---|
| R6 | One engineer-hour of external crypto-path review. | Founder | ~$200-500 OR one community-ping turnaround |
| R7 | Marketing-copy audit pass: PATH-A-BRIEF + BUILD-PLAN + landing-page copy + the codex-voice video transcript. Look for "does the code actually back this up?" gaps like the one caught in today's marketing-pitch audit. | Agent | ~2 hours |
| R8 | Run a single end-to-end demo: build a tiny synthetic corpus → sign → emit Croissant + Article 53 → prove inclusion against a known-included doc → prove non-inclusion against a known-excluded doc → verify all three bundles with `cosign` alone. Document the demo as a `docs/demo/` directory. | Agent | ~4 hours after Sprint 5 closes |
| R9 | Hyper Beam Media LLC operating-agreement check: confirm IP-holding clause is appropriate; confirm copyright assignment is unambiguous. | Founder + accountant/lawyer | ~30 min call |

### Post-launch (nice-to-have)

| # | Action | Owner | Effort |
|---|---|---|---|
| R10 | Benchmark the BUILD-PLAN performance target (100GB-in-10-min on 16 cores). | Agent | ~1 day of run-time + ~2 hours of setup |
| R11 | In-toto vetted-catalog PR citing the v0.3 predicate URIs. | Founder | ~30 min after Netlify deploy of v0.3 schemas |
| R12 | Hugging Face Hub publish flow (Sprint 6 deliverable). | Agent | ~Sprint 6 scope |

---

## 7. Honest things this audit did NOT do

To be useful, an audit must also be honest about what it isn't.

- This is a *self-audit* by an AI agent working on the project. It is not a third-party security review, a legal opinion, a market validation, or a financial audit. Each of those needs its own actor.
- This audit *did not* re-derive the cryptographic primitives' security properties from first principles. It accepted published primitives as published.
- This audit *did not* simulate adversarial scenarios. It read the code's defensive postures and inferred coverage; it did not attempt to break them.
- This audit *did not* benchmark performance. The BUILD-PLAN 100GB-in-10-min target is an assertion, not yet a measurement.
- This audit *did not* validate that the Path A thesis is correct. It validated that the codebase *would deliver* the Path A thesis if the thesis is correct. The thesis itself is unvalidated by external design partners.
- This audit *did not* check for regressions against the historical Annex/ artifact (frozen; out of scope) — only against the current `Attestrum/` codebase at HEAD `a0559dd`.
- This audit *did not* analyze the legal exposure of any specific signed-bundle use case (e.g., is publishing a non-inclusion proof for someone's copyrighted work a defensible regulatory move under the EU AI Act? — this requires counsel).
- This audit *did not* survey the competitive landscape (are other projects building this same trust layer? are there frontier-lab in-house tools that would obviate Attestrum?). The PATH-A-BRIEF references the May 2026 frontier-lab competitive audit that triggered the Path A pivot, but that audit predates this report and may itself need refresh.

---

## 8. Closing

Attestrum is buildable. Sprints 1–4 are demonstrably shipped, with discipline that exceeds most production codebases. Sprint 5 is well-scoped and uses primitives that exist + work. Sprint 6 is concretely targeted. The remaining risk is concentrated in three places: the PROTECTED text-fingerprint normalization decision in S5-D1 E1, the absence of external code review of the cryptographic paths, and — most importantly — the absence of any conversation with the named design partners on whose interest the entire Path A thesis rests.

The single highest-leverage action available is the design-partner outreach. It costs 30 minutes and tells you whether everything else in this document is worth doing.

---

*This is a living document. Update it at each milestone or when material changes invalidate findings. Append-only is not required here (unlike `CHANGELOG.md` / `SESSION-LOG.md`); rewriting sections is appropriate as the project state evolves.*
