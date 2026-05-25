# CLAUDE.md — Attestrum Project Standing Rules

This file is the standing rulebook for any Claude Code agent working in the Attestrum repository. It is read at the start of every session and re-read whenever the agent is uncertain about a process question. If a rule here conflicts with a rule in `BUILD-PLAN.md` or `PATH-A-BRIEF.md`, the more specific document wins for its scope; for anything outside those scopes, this file wins.

---

## 0. Identity And Mode

You are a Claude Code agent working on **Attestrum** — a deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles. The project pivoted in May 2026 from a frontier-lab compliance pitch to Path A: the trust layer for open AI training data, aimed at AI2, Pleias, EleutherAI, Black Forest Labs, Mozilla Data Collective, and Hugging Face dataset publishers.

You are running with `--dangerously-skip-permissions`. That means you can do real damage. Slow down. Plan first. Confirm before destructive operations. The flag exists so you don't ask permission 200 times per session, not so you can skip thinking.

**Default mode is plan mode.** You do not create files, run shell commands, or edit source code until the founder explicitly approves a plan for the work in front of you. "Go" must be a clear word from the human. "Sounds good, what's next" is not "go." Ask if uncertain.

---

## 1. The Three Documents That Govern This Project

Read these three in this exact order at the start of every session:

1. **`BUILD-PLAN.md`** (v0.1.1) — the original technical kickoff document. Sets the language (Rust), the cryptographic primitives (BLAKE3, RFC 6962 Merkle, Sigstore Bundle v0.3, in-toto Statement v1), the workspace layout, the determinism testing harness, and the signal parsers. Everything in here is canonical unless `PATH-A-BRIEF.md` explicitly overrides it.
2. **`PATH-A-BRIEF.md`** (v0.3.0) — the Path A delta. Adds the `attestrum prove` workflow, the fingerprinting crate, the Hugging Face publish flow, two new attestation predicate types, and the diagram-first CI gate. Sprint 6 of `BUILD-PLAN.md` is fully replaced by Sprint 6 in this document.
3. **This file (`CLAUDE.md`)** — process rules, not technical content. Diagram-first, plan-first, session-logging, protected systems, what-not-to-touch.

If you find a contradiction between the three documents, stop and surface it. Do not silently pick one. The founder decides.

---

## 2. Diagrams Before Code — Non-Negotiable

For any new module, CLI subcommand, public data structure, error path, or multi-party flow, a Mermaid diagram MUST exist in `docs/diagrams/<sprint-or-area>/<topic>.md` BEFORE production code is written for that unit of work.

**Rules:**

- **Mermaid is the source of truth.** No PlantUML, no draw.io, no SVG. Mermaid is the only authoring format for diagram files under `docs/diagrams/`. GitHub renders Mermaid natively in PRs — that's the canonical review path. **PNG renders MAY be generated as derived local artifacts in `/diagrams-png/` (gitignored, never committed, never hand-edited)** for use in slides, partner emails, or print. The renderer is `tools/render-diagrams.sh`; run it after adding or modifying any `docs/diagrams/**/*.md` so your local PNGs stay in sync with the Mermaid sources. PNGs are never the source of truth and never gate the build — the Mermaid file is.
- **Right diagram type for the situation:**
  - `flowchart` for pipelines, dependency graphs, decision trees.
  - `stateDiagram-v2` for state machines, lifecycles, signing flows.
  - `sequenceDiagram` for multi-actor or network flows (OIDC, Hub push, takedown notify).
  - `classDiagram` for stable public Rust APIs.
  - `erDiagram` for on-disk schemas, Parquet column layouts, RocksDB key spaces.
- **Frontmatter is mandatory.** Every diagram file starts with:

  ```yaml
  ---
  title: "<short imperative description>"
  models: "<the code or spec this diagram describes>"
  source_of_truth: code      # or: diagram, spec
  last_verified: <commit SHA short> <YYYY-MM-DD>
  diagram_type: flowchart    # or stateDiagram-v2, sequenceDiagram, classDiagram, erDiagram
  ---
  ```

- **`source_of_truth: code`** means the code is authoritative; the diagram is a derived view and must be re-verified when the code changes.
- **`source_of_truth: diagram`** means the diagram is the contract code must implement (used in the planning phase before code exists). Flips to `code` once implementation lands.
- **`source_of_truth: spec`** means an external specification is authoritative (RFC 6962, in-toto v1, Sigstore Bundle v0.3, Article 53 template). Drift means our implementation is wrong, not the spec.

**Drift handling.** Diagram-vs-code drift is a build break. If you change code in a way that affects the behavior a diagram describes, update the diagram in the SAME commit. The CI diagram-linter catches stale `last_verified` SHAs and dangling references and will block the merge.

**ASCII diagrams in code comments are allowed** for local doc-comment use (`/// [Fetcher] -> [Hasher] -> [Manifest]`). For any standalone diagram file under `docs/diagrams/`, Mermaid is the only allowed format.

---

## 3. Plan-First Gate — The Discipline That Makes Everything Work

You start every feature, fix, or refactor in plan mode. In plan mode:

- You do not run `cargo new`, `cargo add`, `git commit`, `git push`, or any shell command that mutates the workspace.
- You do not create files outside `docs/diagrams/<sprint-or-area>/`.
- You do not edit any file in `crates/`, `tools/`, `tests/`, `.github/`, or the workspace root.
- You **may** read any file in the repo.
- You **may** draft Mermaid diagrams in `docs/diagrams/<sprint-or-area>/` once a sprint plan is approved.

**The transition out of plan mode requires explicit human approval.** A message that says "approved, proceed" or "go" or "ship it" lifts the gate. Anything ambiguous ("sounds good," "looks fine," "what's next") does NOT lift the gate. Ask if uncertain. Better to ask once than to scaffold a wrong directory tree.

**The loop per feature (canonical):**

1. Enter plan mode (declared explicitly).
2. Propose the plan as numbered commits, each small enough to review in one sitting.
3. Apply the 10-star CEO ladder from Tokenmaxxing Principles §3:
   - "What would 6-star, 8-star, 10-star versions look like?"
   - "Which 10-star moves deliver 10x value for 2x effort?"
   - "Name the user for each addition. If the user is 'everyone' or 'future enterprise,' cut it."
4. Revise the plan based on the ladder answers.
5. Draft the Mermaid diagrams the work requires. Place each in `docs/diagrams/<sprint-or-area>/`.
6. Run engineering review of your own plan and diagrams. Look for bugs, edge cases, missing tests, integration risks, user-surprising behavior, and diagram-vs-plan drift. Revise.
7. Wait for human approval of the plan + diagrams.
8. On approval, exit plan mode. Execute commits in order. Append session entry after each commit (see §6).
9. After implementation, run real-browser QA via Playwright MCP if the change has a UI surface (see §9).

---

## 4. Protected Systems — Require Explicit Approval

These subsystems are stable enough that touching them risks corpus-incompatible breakage. Once each has shipped at v0.0.4 or later, modifying any of them requires explicit founder approval in the commit message footer.

- **`crates/attestrum-merkle/`** — RFC 6962 binary Merkle over BLAKE3. Determinism foundation. A wrong byte here invalidates every signed bundle the project has ever issued.
- **`crates/attestrum-attest/` predicate types** — the three URIs `https://attestrum.com/attestation/{training-corpus,inclusion-proof,non-inclusion-proof}/v0.3`. Schema changes require a version bump (`v0.4`), a migration document, and an in-toto vetted catalog re-submission.
- **`crates/attestrum-cas/` directory layout** — anything under `.attestrum/objects/`, `.attestrum/cas/`, `.attestrum/manifests/`. A layout change is a corpus-incompatible event requiring a major version bump.
- **`crates/attestrum-ledger/` tile layout** — append-only by definition. Never rewrite a tile. Never delete a leaf. Witness mode changes require approval.
- **`tests/golden/article53/`** — the EU Article 53 template golden files. Regenerating without visually verifying the output against the Commission's published template is a release-blocking error.
- **`crates/attestrum-fingerprint/` text normalization** — once text fingerprints are committed, changing the tokenization (NFC, lowercase, whitespace collapse) invalidates every inclusion proof emitted so far.

Approval format for touching a protected system:

```
<commit subject>

<commit body>

Protected-system-change: approved-by=<founder name> on=<YYYY-MM-DD>
Reason: <why this change is necessary>
Migration: <link to migration doc or "n/a — backward compatible">
```

---

## 5. The Diagram-First CI Gate

A custom Rust binary at `tools/diagram-linter/` enforces the diagram-first rule on every PR. Before every commit, run it locally:

```bash
cargo run -p diagram-linter -- check --strict
```

The linter checks five things. Each is a hard fail:

1. **Mermaid parse.** Every fenced ```mermaid block in every file under `docs/` parses cleanly via `mmdc`.
2. **Frontmatter present.** Every `docs/diagrams/**/*.md` has all four required keys (`title`, `models`, `source_of_truth`, `last_verified`).
3. **`last_verified` fresh.** The SHA in `last_verified` is within the last 30 commits OR within the current PR's commit range.
4. **Forward references resolve.** Every Rust identifier, file path, or endpoint named in a diagram exists in the workspace.
5. **Reverse references resolve.** Every `pub` item in `crates/**/src/lib.rs` and `crates/**/src/**/mod.rs` is referenced by at least one diagram (generated code and `#[doc(hidden)]` items are exempt).

A failing linter is a failing build. Fix the diagram or fix the code in the same commit.

---

## 6. Session Records — CHANGELOG.md And SESSION-LOG.md

At every commit, append a session entry to BOTH `CHANGELOG.md` and `SESSION-LOG.md`. Same entry, both files.

```markdown
## [YYYY-MM-DD] — <task or commit subject>
- **Files changed**: <paths, comma-separated; "many" + a one-line summary if > 8>
- **Diagrams touched**: <paths under docs/diagrams/, or "none">
- **Summary**: <one paragraph of what changed and why>
- **Findings**: <surprises, decisions made, deferred work, anything the founder should know>
- **Open questions**: <anything you want the next session to address>
- **Tokens used**: <approximate, if known>
```

**Why both files?** `CHANGELOG.md` is the user-facing release narrative; entries get consolidated into release notes at each version bump. `SESSION-LOG.md` is the working log; it preserves the raw session-by-session record including dead ends, deferred work, and decisions that didn't make the changelog. Future-you and future agents read both for context.

**Never delete history from either file.** Append-only. If a decision was wrong, write a new entry explaining the reversal — don't rewrite the old one.

---

## 7. Build And Test Discipline

Before EVERY commit, all of the following must pass. No exceptions, no "I'll fix it in the next commit," no "the CI will catch it."

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p diagram-linter -- check --strict
```

If any of those fail, fix it before committing. Never disable a failing test to get a green light. Never `#[allow(...)]` a clippy lint without a comment explaining the specific case. Never skip the linter "just this once."

**Determinism.** This project depends on byte-identical builds across Linux x86, Linux ARM, macOS, and Linux musl. Sources of non-determinism are bugs:

- All map iteration through `BTreeMap` or explicitly sorted `Vec`.
- All timestamps from a single `--source-date-epoch` parameter (Reproducible Builds convention).
- No floating-point arithmetic in any hash or Merkle path.
- `serde_json` configured with sorted-keys feature.
- Parquet writer pinned to `zstd` level 3, no dictionary fallback heuristics.

If a test passes on your machine but fails in CI's determinism matrix, the test is finding a real bug. Fix the bug, don't relax the test.

---

## 8. Dependency Discipline

Do not add a crate without explicit approval. The dependency list in `BUILD-PLAN.md` §6.2 and `PATH-A-BRIEF.md` Part 2 is the canonical set. If you need a crate not listed there:

1. Surface the need in the plan, not in code.
2. Propose the crate by name, version, license, and reason.
3. Wait for approval before `cargo add`.

**Exceptions explicitly forbidden without approval:**

- No GPL or AGPL dependencies. Apache-2.0, MIT, BSD, MPL-2.0, Unlicense, CC0 only. Anything else stops the PR. **Approved transitive-only exception**: `Unicode-3.0` is allowed for transitive Unicode-Consortium data tables (used industry-wide by `unicode-ident` via `serde_derive`). The exception applies only to transitive dependencies, not to direct workspace deps. Any other transitive license not on the base list still stops the PR — surface to the founder.
- No `unsafe` outside of FFI shims to vetted C libraries. If you find yourself wanting `unsafe`, surface it.
- No git-pinned dependencies except where the brief explicitly approves one (the `hf-hub` upload API pin is the only currently approved git rev; see `PATH-A-BRIEF.md` Part 2.3).
- No alpha / pre-release versions. Crates must have at least one stable release, and we pin minor versions in the workspace `Cargo.toml`.

**License inventory.** When you add a dependency, append a row to `docs/license-inventory.md` with crate name, version, license SPDX ID, and the date added.

---

## 9. UI / Browser-Surface Changes

If a change introduces or modifies a UI surface — the static `verify.html` page, the dataset card README rendering, or the eventual dashboard — run real-browser QA via Playwright MCP before declaring the change done.

**One-time MCP setup** (run once per machine, not per project):

```bash
claude mcp add -s user playwright npx @playwright/mcp@latest
```

**After every UI change, run this QA suite:**

> Use the Playwright MCP to verify this feature in Chromium. Test:
> 1. The golden path end-to-end.
> 2. The obvious edge cases: empty inputs, invalid inputs, missing fields, error states.
> 3. At least one deliberately broken input that should be rejected — confirm rejection with a useful error message.
> 4. The mobile viewport (375x667). Does the UI break at small width?
> 5. Browser console errors. Read the console; any uncaught errors fail the QA.
> Report: (a) golden path pass/fail, (b) screenshots of unexpected state, (c) numbered bug list with one-sentence fixes, (d) console error transcript.

Attestrum's UI surface is small (static `verify.html` + dataset card README), but it's the user-facing trust signal. A broken verify page costs more credibility than a broken backend test.

---

## 10. Communication Style

- **Be concise.** Long preambles waste tokens and bury the answer. Lead with the conclusion, then the reasoning.
- **Surface contradictions immediately.** If the founder asks you to do something that conflicts with a rule in this file or in the build plan, say so. Don't try to silently reconcile.
- **Push back when you have a real reason.** If you think a planned approach is wrong, say so with specifics — not "are you sure?" but "the approach in step 3 will fail under condition X because of reason Y; I'd suggest Z instead."
- **Don't fawn.** No "great question!" No "I'd be happy to!" No "absolutely!" Get to the substance.
- **Don't apologize for things that aren't errors.** A clarifying question is not an apology-worthy event.
- **Ask one question at a time when blocked.** Bundling three questions into one paragraph forces the founder to triage; one question gets a fast answer.

---

## 11. What This Project Is And Is Not

**Attestrum IS:**

- A deterministic Rust CLI that takes a training corpus and emits a cryptographically verifiable provenance bundle.
- Sigstore-signed, in-toto-attested, Merkle-rooted over BLAKE3.
- Open-source under Apache-2.0 OR MIT (dual-license, contributor's choice).
- Aimed at the willing transparent middle: AI2, Pleias, EleutherAI, Black Forest Labs, Mozilla Data Collective, Hugging Face dataset publishers.
- Built solo with Claude Code as the implementation harness.
- A 90-day MVP under a six-sprint plan.

**Attestrum IS NOT:**

- A frontier-lab compliance tool. That pitch was killed by competitive audit in May 2026 because frontier labs are litigating to keep their corpus details ambiguous and would not buy a tool that makes them auditable.
- A registry. We don't host fingerprints, don't run a witness service in v1 (only optionally federate with Rekor or Hugging Face), don't operate a hosted SaaS in v1.
- A two-sided market. The buyer is the publisher of the corpus, not the rightsholder asking "was my work used." That's a v2 product line.
- An ML research project. There is no model training in scope. Fingerprinting uses published algorithms (BLAKE3, ISCC, pHash, MinHash); we do not invent new ones.
- A litigation eDiscovery tool. That's an adjacent v2 opportunity through Thomson Reuters / RELX / Relativity, not v1.
- A general-purpose data versioning system. We are not building Git for data, not building DVC, not building lakeFS. We're building a deterministic compiler that emits a specific kind of signed artifact.

If asked to add something that would push the scope toward any of the "is not" items above, surface the conflict before scoping the work.

---

## 12. Acquirer-Optionality Hygiene

The acquisition narrative depends on Attestrum becoming substrate, not a branded silo. Decisions that preserve optionality:

- **Public type URIs only.** The Sigstore bundle, the in-toto Statement, the Croissant JSON-LD, the CycloneDX ML-BOM all use their canonical public URIs. The string "Attestrum" appears only in the predicate URI prefix (`attestrum.com/`) and in the informational `builderVersion` field — never in emitted format structure.
- **Domain ownership migratability.** The `attestrum.com` domain is registered. If an acquirer wants the predicates moved to a vendor-neutral namespace, the in-toto attestation framework's New Predicate Guidelines workflow defines a rename path.
- **No vendor lock-in.** Every artifact is verifiable with `cosign v3+ verify-blob-attestation --new-bundle-format` and no Attestrum install. The static `verify.html` page works without Attestrum. The Croissant JSON-LD validates against the public schema. The Article 53 template matches the Commission's exact format.
- **Hub-publish is one target among several.** `attestrum publish` supports Hugging Face primary, GitHub Releases fallback, and static-bundle output for Zenodo or self-hosting. No single platform dependency.

When in doubt: optimize for "any acquirer could run this without breaking the OSS users," not "we tightly integrate with company X."

---

## 13. Founder Context

The founder is a solo developer running on a MacBook Air with Claude Code as the primary implementation tool. Existing pattern is spec-driven development: detailed `.md` prompt files, plan-first gates, CHANGELOG/SESSION-LOG entries at every commit. Strong build-system and pipeline background (the AI training-data parallels to MLS sync, photo pipelines, R2 storage, cron orchestration are direct). No ML research background needed for this project. Comfortable with Rust at a pragmatic-not-pedantic level — write idiomatic Rust, don't write `unsafe` showcases.

The founder is not a lawyer. Do not generate legal opinions about Article 53, CDSM Article 4(3), or any active copyright case. When a regulatory question comes up, point to the citation, restate what the spec says, and let the founder decide what to do with it.

---

## 14. Anti-Patterns Specifically Forbidden

These have burned the founder before. Don't repeat them.

- **Silent scope creep.** Don't add "while I'm in here" changes outside the approved plan. If you see something to fix, surface it; don't fix it in the same commit.
- **TODO comments without owners.** Either ship it, file a tracked issue with a link in the comment, or remove the code path entirely. `// TODO: handle this later` is forbidden.
- **Dead code paths.** If a branch is unreachable, delete it. If it's reachable but unused, write the test that exercises it. No "I'll wire this up later" stubs.
- **Eager generalization.** Do not write a trait abstraction for one concrete type "in case we need it later." Write the concrete type; extract the trait when the second implementation appears.
- **Premature optimization.** No `unsafe`, no SIMD intrinsics, no hand-rolled allocators, no custom hash maps in v1. The deterministic Rust baseline is fast enough for a 100GB corpus in under 10 minutes on a 16-core box. Hit that target with safe code first.
- **Implicit network calls.** Every function that hits the network is documented as such. The default for any new function is "no network." If it needs network, it takes an explicit client parameter and returns an error type that includes a network variant.
- **Untested error paths.** Every `Result::Err` branch has at least one test that exercises it. Error paths that exist only to satisfy the compiler are dead code (see above).
- **"It works on my machine."** If the CI determinism matrix disagrees, the CI is right. Find the source of non-determinism and fix it.

---

## 15. When You Are Uncertain

The default response when you don't know what to do is **ask before acting**. Specific patterns:

- Don't know which approach the founder prefers? Lay out two options with trade-offs and ask which.
- Don't know if a change is in scope? Quote the relevant sentence from `BUILD-PLAN.md` or `PATH-A-BRIEF.md` and ask.
- Don't know if a dependency is approved? Cite the dependency list location and ask.
- Don't know if you're in plan mode or execution mode? Assume plan mode. Ask.
- Don't understand why a test is failing? Show the test output and ask before "fixing" the test.

Asking once costs 30 seconds. Building the wrong thing costs hours. The trade is always favorable.

---

## Quick Reference Card

| Situation | Action |
|---|---|
| Starting a new feature | Enter plan mode. Read `BUILD-PLAN.md`, `PATH-A-BRIEF.md`, this file. Confirm scope. |
| Before any code change | Mermaid diagram first under `docs/diagrams/<area>/`. Frontmatter required. |
| Before any commit | Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo run -p diagram-linter -- check --strict`. |
| After every commit | Append session entry to `CHANGELOG.md` AND `SESSION-LOG.md`. |
| Touching a protected system | Surface to founder. Get explicit approval in commit message footer. |
| Adding a dependency | Surface name, version, license, reason. Wait for approval. Update `docs/license-inventory.md`. |
| UI surface change | Run Playwright MCP QA before declaring done. |
| Uncertain about anything | Ask before acting. |
| Tempted to skip a rule "just this once" | Don't. Either commit the bypass in writing or apply the rule. |

---

*Last updated: 2026-05-24. Attestrum v0.3.0 (rebrand from Annex codename). Tokenmaxxing Principles v2 informs §2, §3, §6, §9.*
