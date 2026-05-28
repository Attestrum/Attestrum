# CLAUDE.md — Attestrum Project Standing Rules

This file is the standing rulebook for any Claude Code agent working in the Attestrum repository. It is read at the start of every session and re-read whenever the agent is uncertain about a process question.

**This file is authoritative.** If anything in a handoff document, kickoff prompt, plan file under `~/.claude/plans/`, agent memory, or any other agent-facing artifact conflicts with the current state of this file (gate count, file paths, §6 logging policy, anti-patterns, dependency rules, etc.), THIS FILE WINS. Surface the conflict to the founder before acting. Do not silently reconcile.

---

## 0. Identity And Mode

You are a Claude Code agent working on **Attestrum** — a deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles. The project pivoted in May 2026 from a frontier-lab compliance pitch to Path A: the trust layer for open AI training data, aimed at AI2, Pleias, EleutherAI, Black Forest Labs, Mozilla Data Collective, and Hugging Face dataset publishers.

You are running with `--dangerously-skip-permissions`. That means you can do real damage. Slow down. Plan first. Confirm before destructive operations. The flag exists so you don't ask permission 200 times per session, not so you can skip thinking.

**Default mode is plan mode.** You do not create files, run shell commands, or edit source code until the founder explicitly approves a plan for the work in front of you. "Go" must be a clear word from the human. "Sounds good, what's next" is not "go." Ask if uncertain.

---

## 0.4 First-Time Setup On A Fresh Clone — Do This Before Your First Commit

These steps are one-time per clone. The protocol's strongest gates depend on them; skipping them silently disables the local-side defenses.

```bash
# 1. Activate the pre-commit hook (runs all six gates per §7 on every commit).
git config core.hooksPath .githooks

# 2. Verify activation.
git config --get core.hooksPath   # must print: .githooks
ls -la .githooks/pre-commit       # must be executable

# 3. Run the six gates once to confirm baseline green-before-change.
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
cargo deny check sources licenses
cargo run -p secret-scanner --release --quiet -- check --all
```

If any of step 3 fails on a fresh clone with no edits, surface that to the founder before proceeding — the baseline has drifted from when this file was last verified.

---

## 0.5 Publication Boundary — What Goes In The Public Repo, What Stays Local

**This repo is PUBLIC at `github.com/Attestrum/Attestrum`.** Every commit you make is immediately visible to the world. Internal research, session transcripts, strategic positioning, audit deliberations, founder personal data, and other-business portfolio content stay LOCAL, outside this repo.

This section is the canonical policy. The protocol layers below (`.gitignore`, the `tools/secret-scanner/` pre-commit gate, the `.githooks/pre-commit` hook, the CI verification job, and GitHub server-side push protection) enforce it mechanically. **Do not rely on the layers being perfect. Re-read this section at every session start and apply judgment.**

### 0.5.1 Public surface (in this repo, anyone can read)

| Path | Class |
|---|---|
| `/crates/`, `/tools/`, `/tests/` | Rust source + linter tests + test fixtures |
| `/docs/diagrams/` | Mermaid architecture diagrams |
| `/docs/migration/`, `/docs/schemas/`, `/docs/research/`, `/docs/license-inventory.md` | Migration / schema / research / license docs |
| `/.github/workflows/` | CI definitions |
| `/Cargo.toml`, `/Cargo.lock`, `/rust-toolchain.toml`, `/rustfmt.toml`, `/clippy.toml`, `/deny.toml` | Build / lint / dep config |
| `/LICENSE-APACHE`, `/LICENSE-MIT` | License files |
| `/README.md`, `/SECURITY.md`, `/CHANGELOG.md`, `/CLAUDE.md`, `/DIAGRAMS-OVERVIEW.md` | Public docs |
| `/.gitignore`, `/.githooks/` | Repo config |

### 0.5.2 Internal-only (NEVER in this repo, ever)

| Path | Class |
|---|---|
| `~/Documents/Claude/Attestrum-internal-notes/` | **The canonical internal-only working directory.** Session reports, audit deliberations, removed historical docs, the verbose pre-public CHANGELOG, the original `BUILD-PLAN.md` + `PATH-A-BRIEF.md`, etc. |
| `~/.claude/plans/` | Claude Code session plan files |
| `~/.claude/projects/-Users-austinmunday-Documents-Claude-Attestrum/memory/` | Claude Code per-project memory |
| `~/Documents/Claude/Annex/` | Annex-era historical repo (pre-rebrand) |
| `~/Documents/Claude/Attestrum-brand-assets/` | Brand asset source files (video / audio / logo) — sibling, outside repo |
| `.playwright-mcp/` (in-repo, gitignored) | Browser MCP session cache |
| `.attestrum/`, `.claude/`, `/diagrams-png/`, `target/` (gitignored) | Local working dirs / build artifacts |
| `/_*` (defensive glob; currently no occupants) | Reserved namespace for future underscore-prefixed local dirs at the repo root |

### 0.5.3 Categorically forbidden in any tracked file

Even if a file is otherwise public-surface, these patterns must never appear in its content:

- **Absolute filesystem paths**: anything matching `/Users/<name>/...` or `/home/<name>/...`. Use repo-relative paths only.
- **Personal email addresses**: `austindmunday@gmail.com` and any other personal email. Use organizational / project email (`security@attestrum.com`) or no email at all.
- **Founder's other-business domains**: `austinmundayrealestate.com`, `loadhog.pro`, `austinsidxcombinator.com`, `austinsradar.com`, `haultickets.com`, `img.tokenmaxxen.com`, and similar. These reveal the founder's broader portfolio; they belong in internal notes, not public docs.
- **Localhost URLs**: `http://127.0.0.1:*`, `http://localhost:*`, and similar — reveal local dev-setup details. If a diagram needs to describe local infrastructure abstractly, do so without naming a specific port.
- **Session transcripts, handoff docs, GTM docs, multi-reviewer audit deliberations, decision-process meta-narrative**. These belong in internal-notes.
- **Real API keys / tokens / private keys / JWTs / certificates**. The `tools/secret-scanner/` pre-commit gate (CLAUDE.md §7 sixth gate) catches the known patterns; do not rely solely on it.

### 0.5.4 Anti-patterns specifically forbidden for agents

- **`git add -A` / `git add .` is forbidden.** Always name files explicitly. The most common failure mode is an agent staging an untracked file that was never meant to be in the repo.
- **Never copy a file from outside the repo into the repo.** If an internal-note becomes interesting enough to publish, rewrite it from scratch as a public-facing doc, then commit the new file — do not `cp` from `~/Documents/Claude/Attestrum-internal-notes/`.
- **Never echo a secret or path into a tracked file**. If you need to demonstrate a credential format, use an obvious placeholder (`<API_KEY>`, `sk-...REPLACE_ME...`).
- **Never bypass the pre-commit hook.** The `.githooks/pre-commit` hook intentionally has no env-var override. To skip it requires editing the hook file, which is a deliberate act subject to founder review.
- **Never reference `~/.claude/plans/<file>.md` paths in tracked files.** Plan files live outside the repo; they are session-internal context, not project documentation.

### 0.5.5 Naming cadence — three tiers signal "off-limits-to-push" at a glance

The publication boundary uses a three-tier naming convention so the filesystem layout itself signals public-vs-private without anyone having to consult `.gitignore` or this file. When you create a new persistent location, place it in the tier that matches its purpose:

| Tier | Convention | When to use | Examples |
|---|---|---|---|
| **1. External sibling** (strongest) | `~/Documents/Claude/Attestrum-<purpose>/` — outside the repo, alongside it | Internal notes, brand assets, anything too persistent or large to recreate; content the founder needs to keep but never publish | `Attestrum-internal-notes/`, `Attestrum-brand-assets/` |
| **2. In-repo dotfile prefix** | `.<name>/` — inside the repo, gitignored | Tool caches and CLI working dirs that MUST live in-repo for the tool to function | `.attestrum/` (CLI working dir), `.claude/`, `.playwright-mcp/`, `target/` (Rust build) |
| **3. In-repo underscore prefix** | `_<name>/` — inside the repo, caught by `/_*` defensive glob in `.gitignore` | Project-local working dirs that don't fit the dotfile convention. Currently empty — reserved namespace | (no current occupants) |

**Tracked exceptions to the dotfile convention**: `.github/` (CI workflows) and `.githooks/` (pre-commit hook) are dotfile-named but are TRACKED and public. Don't `git rm` these by pattern.

**`~/.claude/plans/` is a fourth implicit tier** — session plan files live in the founder's home directory under Claude Code's per-user plans store. Not project-specific; never moved into the repo regardless of how mature the plan becomes.

When in doubt: default to tier 1 (external sibling). The physical separation is uncatchable by any `git add` mistake.

### 0.5.6 If you find a leak

If you discover a file or piece of content that violates §0.5.1-§0.5.5 already in the public tree:

1. **Stop other work.** Surface to the founder before doing anything destructive.
2. **For new leaks** (would land in your current commit): edit the file before committing.
3. **For already-committed leaks** (in `main`): do NOT `git filter-repo` without explicit founder approval. The default is to `git rm` from current state + accept that history retains the content. `git filter-repo` rewrites SHAs and breaks every diagram-linter `last_verified` reference in the repo.
4. **For credential leaks** specifically: rotate the credential first, then handle the repo cleanup. The credential is the priority, not the file.

---

## 1. The Document That Governs This Project

This file (`CLAUDE.md`) is the canonical rulebook: process rules, not technical content. Diagram-first, plan-first, session-logging, protected systems, what-not-to-touch.

Technical context — the cryptographic primitives (BLAKE3, RFC 6962 Merkle, Sigstore Bundle v0.3, in-toto Statement v1), workspace layout, sprint schedule, signal parsers — is derivable from the current code, the `docs/diagrams/` tree, and `CHANGELOG.md`. The original kickoff document and the Path A pivot brief are retained as local-only notes outside the repo for project memory; they are not part of the public source-of-truth set.

If you find a contradiction between this file and the current code, stop and surface it. Do not silently pick one. The founder decides.

---

## 2. Diagrams Before Code — Non-Negotiable

For any new module, CLI subcommand, public data structure, error path, or multi-party flow, a Mermaid diagram MUST exist in `docs/diagrams/<sprint-or-area>/<topic>.md` BEFORE production code is written for that unit of work.

**Rules:**

- **Mermaid is the source of truth.** No PlantUML, no draw.io, no SVG. Mermaid is the only authoring format for diagram files under `docs/diagrams/`. GitHub renders Mermaid natively in PRs — that's the canonical review path. **PNG renders MUST be generated as derived local artifacts in `/diagrams-png/` (gitignored, never committed, never hand-edited)** every time you add or modify a file under `docs/diagrams/**/*.md`. The renderer is `tools/render-diagrams.sh`; run `bash tools/render-diagrams.sh` from the repo root after your diagram edits land but before you declare the task done. The founder uses the PNGs for visual review in internal notes; if you skip the render, the founder sees stale or missing images. PNGs are never the source of truth and never gate the build — the Mermaid file is — but generating them is no longer optional for agents.
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
5. Draft the Mermaid diagrams the work requires. Place each in `docs/diagrams/<sprint-or-area>/`. Then run `bash tools/render-diagrams.sh` to regenerate `/diagrams-png/` so the founder can visually review each diagram alongside the Mermaid source.
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

A custom Rust binary at `tools/diagram-linter/` enforces the diagram-first rule on every PR. Before every commit, run it locally (this is gate 4 of the six-gate ritual in §7):

```bash
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
```

The `--root docs/diagrams` argument matches CI's invocation (`.github/workflows/ci.yml`); omitting it walks a different tree and produces inconsistent results.

The linter runs six checks. Each is a hard fail:

1. **Mermaid parse.** Every fenced ```mermaid block in every file under `docs/` parses cleanly via `mmdc`.
2. **Frontmatter present.** Every `docs/diagrams/**/*.md` has all four required keys (`title`, `models`, `source_of_truth`, `last_verified`).
3. **`last_verified` fresh.** The SHA in `last_verified` is within the last 30 commits (excluding docs-only commits to `CHANGELOG.md` / `SESSION-LOG.md` per `DOCS_ONLY_EXCLUDES`) OR within the current PR's commit range.
4. **Forward references resolve.** Every Rust identifier, file path, or endpoint named in a diagram's `models:` field exists in the workspace (enforced only for `source_of_truth: code` diagrams).
5. **Reverse references resolve.** Every `pub` item in `crates/**/src/lib.rs` and `crates/**/src/**/mod.rs` is referenced by at least one diagram (generated code and `#[doc(hidden)]` items are exempt).
6. **Drift.** When a code file named in a `source_of_truth: code` diagram's `models:` field is staged for commit, the diagram itself must also be staged in the same commit (typically just bumping `last_verified` to certify the diagram is still accurate against the new code).

A failing linter is a failing build. Fix the diagram or fix the code in the same commit.

---

## 6. Session Records — CHANGELOG.md (tracked) + SESSION-LOG.md (local-only)

`CHANGELOG.md` is the user-facing release narrative tracked in the public repo. Append a release-relevant entry at every commit that lands user-visible behavior, dependency changes, or notable architectural moves. Routine refactors and internal cleanups can be omitted from CHANGELOG.md; rely on the commit message + git log for those.

`SESSION-LOG.md` is the working log — same shape as the prior tracked SESSION-LOG.md, but now kept LOCAL-ONLY at `~/Documents/Claude/Attestrum-internal-notes/SESSION-LOG.md`. It preserves the raw session-by-session record including dead ends, deferred work, token usage, and decisions that didn't make the changelog. Append to it at every commit so future-you and future agents have the full context. It is not pushed to GitHub.

CHANGELOG.md entry shape (release-oriented):

```markdown
## [version or YYYY-MM-DD] — <user-facing summary>
- <bullet of what changed for users / contributors>
- <bullet of what changed for users / contributors>
```

SESSION-LOG.md entry shape (working log, local-only):

```markdown
## [YYYY-MM-DD] — <task or commit subject>
- **Files changed**: <paths, comma-separated; "many" + a one-line summary if > 8>
- **Diagrams touched**: <paths under docs/diagrams/, or "none">
- **Summary**: <one paragraph of what changed and why>
- **Findings**: <surprises, decisions made, deferred work, anything the founder should know>
- **Open questions**: <anything you want the next session to address>
- **Tokens used**: <approximate, if known>
```

**Never delete history from either file.** Append-only. If a decision was wrong, write a new entry explaining the reversal — don't rewrite the old one.

### 6.1 Push Cadence — Local And Remote Are Always In Sync

Every local commit is pushed to `origin/main` immediately after the local commit lands. No commit sits unpushed except briefly during a deliberate multi-commit landing sequence.

**Current remote**: `https://github.com/Attestrum/Attestrum.git`. Hosted by the `Attestrum` GitHub org (created 2026-05-25, owned by Hyper Beam Media LLC).

**Workflow per commit, in order:**

1. Run pre-commit gates (§7).
2. `git add <specific paths>` (never `-A` or `.`).
3. `git commit -m '...'` (with CHANGELOG.md entry staged in the same commit if the change is release-relevant per §6; the local-only SESSION-LOG.md gets appended outside the commit).
4. `git push origin <current-branch>` (today this is always `main`).

The push is part of the commit ritual, not a separate ceremony. After the local commit lands, push immediately.

**Why push every time:**

- The remote is the canonical backup against local disk loss.
- CI on push:main (the `ci.yml` fmt/clippy/test/cargo-deny job, the `determinism.yml` 4-target byte-identity matrix, the `cosign-interop.yml` Sigstore round-trip) validates against the actual GHA environment, not the local dev box. Local-green ≠ CI-green; the only way to know is to push and watch.
- Solo-developer workflows that batch pushes lose the GHA validation feedback loop and silently accumulate environment-drift bugs that surface as a cascade at the next push.
- Future external collaborators (and future-self) see canonical history without a delayed-push surprise.

**Acceptable batched-push cases** (rare):

- A deliberate multi-commit landing sequence where commit B depends semantically on commit A and you want both to land at the remote together. Push all commits in the sequence with one `git push`, not per-commit.
- A fix-forward immediately following a commit you just realised was wrong. Hold the push briefly while you draft the fix-forward commit; push both together.

**Never acceptable:**

- Sitting on a green local commit indefinitely "to test more locally first." If the pre-commit gates passed, push.
- `git push --force` to `main` — overwrites the remote canonical history. Only with explicit founder approval and a written-down recovery plan.
- `git commit --no-verify` to skip the local `.githooks/pre-commit` hook. The hook runs all six gates per §7; bypassing it is a §0.5.4 anti-pattern. If a gate fails, fix the underlying issue.
- `git push --no-verify` to skip any server-side hooks that get added later.

---

## 7. Build And Test Discipline

Before EVERY commit, all of the following must pass. No exceptions, no "I'll fix it in the next commit," no "the CI will catch it." These six gates are also wrapped by the git pre-commit hook at `.githooks/pre-commit`; activation is `git config core.hooksPath .githooks` (one-time per clone). The hook strict-blocks with no env-var bypass per CLAUDE.md §0.5.4.

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
cargo deny check sources licenses
cargo run -p secret-scanner --release --quiet -- check
```

If any of those fail, fix it before committing. Never disable a failing test to get a green light. Never `#[allow(...)]` a clippy lint without a comment explaining the specific case. Never skip the linter "just this once." Never `git commit --no-verify` to bypass the hook — that's an explicit CLAUDE.md §0.5.4 anti-pattern.

**Gate split.** `sources` catches unapproved git pins (URLs not in `deny.toml`'s `allow-git` list). `licenses` catches transitive deps whose SPDX license isn't in `deny.toml`'s `allow` list. Both run sub-second locally and are policed by CI's `audit` job. `cargo deny check bans` is omitted because the workspace doesn't populate `[bans].deny` — re-add once any entries land. `cargo deny check advisories` is CI-only because it queries the RUSTSEC index (slow) and currently carries two transitive RUSTSEC reds. Historical rationale and the regression incidents that motivated adding the local gate live in `docs/research/cargo-deny-gates-rationale.md`.

**Determinism.** This project depends on byte-identical builds across Linux x86, Linux ARM, macOS, and Linux musl. Sources of non-determinism are bugs:

- All map iteration through `BTreeMap` or explicitly sorted `Vec`.
- All timestamps from a single `--source-date-epoch` parameter (Reproducible Builds convention).
- No floating-point arithmetic in any hash or Merkle path.
- `serde_json` configured with sorted-keys feature.
- Parquet writer pinned to `zstd` level 3, no dictionary fallback heuristics.

If a test passes on your machine but fails in CI's determinism matrix, the test is finding a real bug. Fix the bug, don't relax the test.

**CI status check.** CI state changes commit-to-commit. Run `gh run list -R Attestrum/Attestrum -L 6` at session start to check current state rather than assuming green.

---

## 8. Dependency Discipline

Do not add a crate without explicit approval. The canonical dependency set is whatever is currently in the workspace `Cargo.toml` lockfile plus what's recorded in `docs/license-inventory.md`. If you need a crate not already present:

1. Surface the need in the plan, not in code.
2. Propose the crate by name, version, license, and reason.
3. Wait for approval before `cargo add`.

**Exceptions explicitly forbidden without approval:**

- No GPL or AGPL dependencies. Apache-2.0, MIT, BSD, MPL-2.0, Unlicense, CC0 only. Anything else stops the PR. **Approved transitive-only exception**: `Unicode-3.0` is allowed for transitive Unicode-Consortium data tables (used industry-wide by `unicode-ident` via `serde_derive`). The exception applies only to transitive dependencies, not to direct workspace deps. Any other transitive license not on the base list still stops the PR — surface to the founder.
- No `unsafe` outside of FFI shims to vetted C libraries. If you find yourself wanting `unsafe`, surface it.
- No git-pinned dependencies except where explicitly approved by the founder (the `hf-hub` upload API pin is the only currently approved git rev — recorded in the workspace `Cargo.toml` `[patch.crates-io]` + the `deny.toml` `allow-git` list).
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

## 11. Project Scope

**Attestrum IS:**

- A deterministic Rust CLI that takes a training corpus and emits a cryptographically verifiable provenance bundle.
- Sigstore-signed, in-toto-attested, Merkle-rooted over BLAKE3.
- Open-source under Apache-2.0 OR MIT (dual-license, contributor's choice). Copyright holder: **Hyper Beam Media LLC**. `LICENSE-APACHE` + `LICENSE-MIT` at the repo root carry the canonical copyright lines; per-file SPDX headers are NOT used.

**Attestrum IS NOT:**

- A frontier-lab compliance tool.
- A registry. v1 doesn't host fingerprints or operate a hosted SaaS (only optionally federates with Rekor or Hugging Face).
- A two-sided market — the buyer is the publisher of the corpus, not the rightsholder asking "was my work used."
- An ML research project. Fingerprinting uses published algorithms (BLAKE3, ISCC, pHash, MinHash); we do not invent new ones.
- A litigation eDiscovery tool.
- A general-purpose data versioning system. We are not building Git for data, DVC, or lakeFS — we're building a deterministic compiler that emits a specific kind of signed artifact.

If asked to add something that would push the scope toward any of the "IS NOT" items above, surface the conflict before scoping the work.

---

## 12. Vendor Neutrality

Every artifact Attestrum emits must verify with standard public tooling — no Attestrum install required. Decisions that preserve neutrality:

- **Public type URIs only.** The Sigstore bundle, the in-toto Statement, the Croissant JSON-LD, the CycloneDX ML-BOM all use their canonical public URIs. The string "Attestrum" appears only in the predicate URI prefix (`attestrum.com/`) and in the informational `builderVersion` field — never in emitted format structure.
- **Domain ownership migratability.** The `attestrum.com` domain is registered. If a future maintainer wants the predicates moved to a vendor-neutral namespace, the in-toto attestation framework's New Predicate Guidelines workflow defines a rename path.
- **No vendor lock-in.** Every artifact is verifiable with `cosign v3+ verify-blob-attestation --new-bundle-format` and no Attestrum install. The static `verify.html` page works without Attestrum. The Croissant JSON-LD validates against the public schema. The Article 53 template matches the Commission's exact format.
- **Hub-publish is one target among several.** `attestrum publish` supports Hugging Face primary, GitHub Releases fallback, and static-bundle output for Zenodo or self-hosting. No single platform dependency.

When in doubt: optimize for "any user can run and verify this without Attestrum installed," not "we tightly integrate with company X."

---

## 13. Legal & Regulatory Questions

When a regulatory or legal question arises (EU AI Act Article 53, CDSM Article 4(3), copyright case law, jurisdictional issues), point to the citation, restate what the specification says, and let the founder decide what to do with it. Do not generate legal opinions.

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
- Don't know if a change is in scope? Quote the relevant sentence from this file (`CLAUDE.md`) or point at the current code / diagram state and ask.
- Don't know if a dependency is approved? Cite the dependency list location and ask.
- Don't know if you're in plan mode or execution mode? Assume plan mode. Ask.
- Don't understand why a test is failing? Show the test output and ask before "fixing" the test.

Asking once costs 30 seconds. Building the wrong thing costs hours. The trade is always favorable.

---

## Quick Reference Card

| Situation | Action |
|---|---|
| Fresh clone (one-time) | `git config core.hooksPath .githooks`, then run the six gates once to confirm baseline green (see §0.4). |
| Handoff / kickoff prompt says something different from this file | THIS FILE WINS. Surface the conflict to the founder before acting (see top-of-file authority anchor). |
| Starting a new feature | Enter plan mode. Read this file (`CLAUDE.md`) + the current code / diagram state. Confirm scope. |
| Before any code change | Mermaid diagram first under `docs/diagrams/<area>/`. Frontmatter required. |
| After adding/modifying any Mermaid diagram | Run `bash tools/render-diagrams.sh` from the repo root to refresh `/diagrams-png/` for founder visual review. PNGs stay gitignored (§2). |
| Before any commit (all six gates) | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams`, `cargo deny check sources licenses`, `cargo run -p secret-scanner --release --quiet -- check`. The `.githooks/pre-commit` hook runs all six automatically. |
| After every commit | Append release-relevant entry to `CHANGELOG.md` (in the commit itself if release-relevant, per §6); append working-log entry to the local-only `SESSION-LOG.md` outside the commit. Then `git push origin main` immediately (per §6.1) — local and remote stay in sync, every commit. |
| Touching a protected system | Surface to founder. Get explicit approval in commit message footer. |
| Adding a dependency | Surface name, version, license, reason. Wait for approval. Update `docs/license-inventory.md`. |
| UI surface change | Run Playwright MCP QA before declaring done. |
| Uncertain about anything | Ask before acting. |
| Tempted to skip a rule "just this once" | Don't. Either commit the bypass in writing or apply the rule. |
| Tempted to `git commit --no-verify` | Don't (§0.5.4 anti-pattern). Fix the failing gate. |

---

*Last updated: 2026-05-27. Attestrum v0.3.0 (rebrand from Annex codename). Tokenmaxxing Principles v2 informs §2, §3, §6, §9. For the per-section change history, see `git log -- CLAUDE.md`.*
