# CLAUDE.md — Attestrum Project Standing Rules

The standing rulebook for any Claude Code agent working in the Attestrum repository. Read at session start; re-read when uncertain about a process question.

**This file is authoritative.** If anything in a handoff document, kickoff prompt, agent memory, or any other agent-facing artifact conflicts with the current state of this file, THIS FILE WINS. Surface the conflict to the founder before acting.

**Handoff prompts must not duplicate this file.** They carry only state NOT in `CLAUDE.md` — current HEAD, current sprint, carry-forward issues, specific files to read, specific commits to land. Policy is referenced by section number, not restated. Restated policy drifts the moment this file is edited.

---

## 0. Identity And Mode

You are a Claude Code agent working on **Attestrum** — a deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles.

You are running with `--dangerously-skip-permissions`. You can do real damage. Slow down. Plan first. Confirm before destructive operations.

**Default mode is plan mode.** You do not create files, run shell commands, or edit source code until the founder explicitly approves a plan. "Go" / "approved" / "ship it" lifts the gate; "sounds good" / "looks fine" / "what's next" does NOT.

Technical context (cryptographic primitives, workspace layout, sprint schedule, signal parsers) is derivable from the current code, `docs/diagrams/`, and `CHANGELOG.md`. If you find a contradiction between this file and the code, stop and surface it.

---

## 0.4 First-Time Setup On A Fresh Clone

```bash
# 1. Activate the pre-commit hook.
git config core.hooksPath .githooks

# 2. Verify activation.
git config --get core.hooksPath        # must print: .githooks
ls -la .githooks/pre-commit            # must be executable

# 3. Run the six gates once to confirm baseline green.
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
cargo deny check sources licenses
cargo run -p secret-scanner --release --quiet -- check --all
```

If step 3 fails on a clean clone, surface it — the baseline has drifted.

---

## 0.5 Publication Boundary — Public Repo vs Local

**This repo is PUBLIC at `github.com/Attestrum/Attestrum`.** Every commit is immediately visible. Internal research, transcripts, strategic positioning, audit deliberations, founder personal data, and other-business portfolio content stay LOCAL. The `.gitignore`, `tools/secret-scanner/` pre-commit gate, `.githooks/pre-commit` hook, CI verification job, and GitHub server-side push protection enforce this mechanically — **do not rely on those layers being perfect.**

### 0.5.1 Public surface

| Path | Class |
|---|---|
| `/crates/`, `/tools/`, `/tests/` | Rust source, linter tests, fixtures |
| `/docs/diagrams/` | Mermaid architecture diagrams |
| `/docs/migration/`, `/docs/schemas/`, `/docs/research/`, `/docs/license-inventory.md` | Migration / schema / research / license docs |
| `/.github/workflows/`, `/.githooks/` | CI definitions, pre-commit hook |
| Workspace config (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`) | Build / lint / dep config |
| `/LICENSE-APACHE`, `/LICENSE-MIT` | License files |
| `/README.md`, `/SECURITY.md`, `/CHANGELOG.md`, `/CLAUDE.md`, `/DIAGRAMS-OVERVIEW.md`, `/.gitignore` | Public docs and repo config |

### 0.5.2 Internal-only (NEVER in this repo)

| Path | Class |
|---|---|
| `~/Documents/Claude/Attestrum-internal-notes/` | **Canonical internal working dir.** Session reports, audit deliberations, removed historical docs, the verbose pre-public CHANGELOG, the original BUILD-PLAN / PATH-A-BRIEF, etc. |
| Claude Code per-project memory store, plan files | Per-project agent memory and session plans (under the agent's local stores) |
| Pre-rebrand historical repo (sibling) | Historical working copy from before the current name |
| `~/Documents/Claude/Attestrum-brand-assets/` | Brand asset source files — sibling, outside repo |
| `.playwright-mcp/`, `.attestrum/`, `.claude/`, `/diagrams-png/`, `target/` | In-repo tool caches / build artifacts (gitignored) |
| `/_*` (defensive glob, currently empty) | Reserved namespace for future in-repo local dirs |

### 0.5.3 Categorically forbidden in any tracked file

- **Absolute filesystem paths**: `/Users/<name>/...` or `/home/<name>/...`. Use repo-relative paths only.
- **Founder's personal email addresses.** Use organizational / project email (`security@attestrum.com`) or no email at all. Literal forbidden form lives in `SENSITIVE-PATTERNS.md` (internal notes).
- **Founder's other-business domains.** Reveal the founder's broader portfolio. Literal forbidden list lives in `SENSITIVE-PATTERNS.md` (internal notes).
- **Localhost URLs**: `http://127.0.0.1:*`, `http://localhost:*`. Describe local infrastructure abstractly without naming a specific port.
- **Session transcripts, handoff docs, GTM docs, multi-reviewer audit deliberations, decision-process meta-narrative.**
- **Real API keys / tokens / private keys / JWTs / certificates.** The `tools/secret-scanner/` gate catches known patterns; do not rely solely on it.

### 0.5.4 Anti-patterns specifically forbidden for agents

- **`git add -A` / `git add .` is forbidden.** Always name files explicitly. Most common failure: an agent stages an untracked file that was never meant to be in the repo.
- **Check `git status --short` before every `git add`.** If a file you didn't edit is dirty, surface it before staging — parallel sessions in the same working tree can drop foreign edits into your commit.
- **Never copy a file from outside the repo into the repo.** If an internal note is interesting enough to publish, rewrite it from scratch — do not `cp` from internal notes.
- **Never echo a secret or path into a tracked file.** Use obvious placeholders (`<API_KEY>`, `sk-...REPLACE_ME...`).
- **Never bypass the pre-commit hook.** It intentionally has no env-var override; skipping requires deliberately editing the hook file.
- **Never reference plan-file paths in tracked files.** Plan files live outside the repo; they are session-internal context.

### 0.5.5 Naming cadence — three tiers signal "off-limits"

| Tier | Convention | When | Examples |
|---|---|---|---|
| **1. External sibling** (strongest) | `~/Documents/Claude/Attestrum-<purpose>/` — outside the repo | Internal notes, brand assets, persistent content never to publish | `Attestrum-internal-notes/`, `Attestrum-brand-assets/` |
| **2. In-repo dotfile prefix** | `.<name>/` — inside the repo, gitignored | Tool caches and CLI working dirs that MUST live in-repo | `.attestrum/`, `.claude/`, `.playwright-mcp/`, `target/` |
| **3. In-repo underscore prefix** | `_<name>/` — caught by `/_*` defensive glob | Project-local working dirs that don't fit the dotfile convention. Currently empty. | (none) |

**Tracked exceptions to the dotfile convention**: `.github/` and `.githooks/` are dotfile-named but PUBLIC and tracked. Don't `git rm` these by pattern.

**Plan files are a fourth implicit tier** — they live under the agent's per-user plans store, never moved into the repo.

When in doubt: default to tier 1. The physical separation is uncatchable by any `git add` mistake.

**Hosted state stores are a separate concern.** Vendor memory stores, vendor session stores, vendor agent registries, vendor knowledge bases — separate tier requiring explicit founder approval. Default answer is no: Attestrum's product thesis is local-controlled deterministic provenance; persisting state into a vendor-controlled store undermines the demo. Propose if it genuinely unlocks something we can't build locally.

### 0.5.6 If you find a leak

1. **Stop other work.** Surface to the founder.
2. **For new leaks** (would land in your commit): edit the file before committing.
3. **For already-committed leaks** (in `main`): do NOT `git filter-repo` without explicit approval. Default is `git rm` from current state + accept history retains the content. `git filter-repo` rewrites SHAs and breaks every diagram-linter `last_verified` reference.
4. **For credential leaks**: rotate the credential first, then handle the repo cleanup. The credential is the priority.

---

## 2. Diagrams Before Code — Non-Negotiable

For any new module, CLI subcommand, public data structure, error path, or multi-party flow, a Mermaid diagram MUST exist in `docs/diagrams/<sprint-or-area>/<topic>.md` BEFORE production code is written.

**Mermaid is the only authoring format.** No PlantUML, no draw.io, no SVG. GitHub renders Mermaid natively. **PNG renders are generated locally to `/diagrams-png/` (gitignored, never committed)** every time you add or modify a `docs/diagrams/**/*.md` file. Run `bash tools/render-diagrams.sh` from the repo root before declaring the task done. The founder uses PNGs for visual review; skipping the render means stale or missing images. PNGs are never the source of truth and never gate the build.

**Right diagram type:**

- `flowchart` for pipelines, dependency graphs, decision trees
- `stateDiagram-v2` for state machines, lifecycles, signing flows
- `sequenceDiagram` for multi-actor or network flows
- `classDiagram` for stable public Rust APIs
- `erDiagram` for on-disk schemas, Parquet layouts, RocksDB key spaces

**Frontmatter is mandatory** in every diagram file:

```yaml
---
title: "<short imperative description>"
models: "<the code or spec this diagram describes>"
source_of_truth: code      # or: diagram, spec
last_verified: <commit SHA short> <YYYY-MM-DD>
diagram_type: flowchart    # or stateDiagram-v2, sequenceDiagram, classDiagram, erDiagram
---
```

- `source_of_truth: code` — code is authoritative; diagram is a derived view; re-verify when code changes.
- `source_of_truth: diagram` — diagram is the contract code must implement (planning phase before code exists); flips to `code` once implementation lands.
- `source_of_truth: spec` — external spec is authoritative (RFC 6962, in-toto v1, Sigstore Bundle v0.3, Article 53 template). Drift means our implementation is wrong, not the spec.

**Drift handling.** Diagram-vs-code drift is a build break. If code change affects a diagram's described behavior, update the diagram in the SAME commit. The CI diagram-linter catches stale `last_verified` SHAs and dangling references.

**ASCII diagrams in code comments are allowed** for local doc-comment use. For any standalone file under `docs/diagrams/`, Mermaid is the only allowed format.

---

## 3. Plan-First Gate

You start every feature, fix, or refactor in plan mode. In plan mode:

- No `cargo new`, `cargo add`, `git commit`, `git push`, or any workspace-mutating shell command.
- No file creation outside `docs/diagrams/<sprint-or-area>/`.
- No edits to `crates/`, `tools/`, `tests/`, `.github/`, or the workspace root.
- You **may** read any file.
- You **may** draft Mermaid diagrams in `docs/diagrams/<sprint-or-area>/` once a sprint plan is approved.

**The transition out of plan mode requires explicit human approval.** Ask if uncertain. Better to ask once than scaffold a wrong directory tree.

**The loop per feature:**

1. Enter plan mode (declared explicitly).
2. Propose the plan as numbered commits, each small enough to review in one sitting.
3. Apply the 10-star CEO ladder: "6-star, 8-star, 10-star versions look like?" → "Which 10-star moves deliver 10x value for 2x effort?" → "Name the user for each addition. If 'everyone' or 'future enterprise,' cut it."
4. Revise the plan.
5. Draft the Mermaid diagrams the work requires. Run `bash tools/render-diagrams.sh` to regenerate PNGs.
6. Run engineering review of your own plan and diagrams. Revise.
7. Wait for human approval.
8. On approval, exit plan mode. Execute commits in order. Append session entry after each (§6).
9. If the change has a UI surface, run real-browser QA via Playwright MCP (§9).

**Beyond plan mode.** Specialized review protocols for high-stakes decisions are documented in internal notes; agents are instructed to surface them when triggers fire.

---

## 4. Protected Systems — Require Explicit Approval

These subsystems are stable enough that touching them risks corpus-incompatible breakage. Modifying any of them at v0.0.4+ requires explicit founder approval in the commit message footer.

- **`crates/attestrum-merkle/`** — RFC 6962 binary Merkle over BLAKE3. Determinism foundation. A wrong byte invalidates every signed bundle ever issued.
- **`crates/attestrum-attest/` predicate types** — the three URIs `https://attestrum.com/attestation/{training-corpus,inclusion-proof,non-inclusion-proof}/v0.3`. Schema changes require a version bump (`v0.4`), a migration document, and an in-toto vetted catalog re-submission.
- **`crates/attestrum-cas/` directory layout** — `.attestrum/objects/`, `.attestrum/cas/`, `.attestrum/manifests/`. Layout change is corpus-incompatible — major version bump.
- **`crates/attestrum-ledger/` tile layout** — append-only. Never rewrite a tile. Never delete a leaf. Witness mode changes require approval.
- **`tests/golden/article53/`** — EU Article 53 template golden files. Regenerating without visually verifying against the Commission's published template is release-blocking.
- **`crates/attestrum-fingerprint/` text normalization** — changing tokenization (NFC, lowercase, whitespace collapse) invalidates every inclusion proof emitted so far.

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

A custom Rust binary at `tools/diagram-linter/` enforces the diagram-first rule. Before every commit (gate 4 of §7):

```bash
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
```

The `--root docs/diagrams` argument matches CI's invocation; omitting it walks a different tree and produces inconsistent results.

Six checks, each a hard fail:

1. **Mermaid parse.** Every fenced ```mermaid block under `docs/` parses cleanly via `mmdc`.
2. **Frontmatter present.** Every `docs/diagrams/**/*.md` has all four required keys.
3. **`last_verified` fresh.** SHA is within last 30 commits (excluding docs-only commits per `DOCS_ONLY_EXCLUDES`) OR within the current PR's commit range.
4. **Forward references resolve.** Every Rust identifier / path / endpoint named in `models:` exists in the workspace (enforced for `source_of_truth: code` only).
5. **Reverse references resolve.** Every `pub` item in `crates/**/src/lib.rs` and `crates/**/src/**/mod.rs` is referenced by at least one diagram (generated code and `#[doc(hidden)]` exempt).
6. **Drift.** When a code file named in a `source_of_truth: code` diagram's `models:` field is staged, the diagram itself must also be staged.

A failing linter is a failing build. Fix the diagram or fix the code in the same commit.

---

## 6. Session Records — CHANGELOG.md (tracked) + SESSION-LOG.md (local-only)

`CHANGELOG.md` is the user-facing release narrative tracked in the public repo. Append a release-relevant entry at every commit that lands user-visible behavior, dependency changes, or notable architectural moves. Routine refactors and internal cleanups can be omitted; rely on the commit message + git log.

`SESSION-LOG.md` is the working log kept LOCAL-ONLY at `~/Documents/Claude/Attestrum-internal-notes/SESSION-LOG.md`. Preserves the raw session-by-session record including dead ends, deferred work, token usage, and decisions that didn't make the changelog. Append at every commit. Not pushed to GitHub.

**Entry shapes:**

```markdown
# CHANGELOG.md (release-oriented)
## [version or YYYY-MM-DD] — <user-facing summary>
- <bullet>

# SESSION-LOG.md (working log, local-only)
## [YYYY-MM-DD] — <task or commit subject>
- **Files changed**: <paths; "many" + summary if > 8>
- **Diagrams touched**: <paths under docs/diagrams/, or "none">
- **Summary**: <one paragraph>
- **Findings**: <surprises, decisions, deferred work>
- **Open questions**: <for the next session>
- **Tokens used**: <approximate>
```

**Never delete history from either file.** Append-only. If a decision was wrong, write a new entry explaining the reversal.

### 6.1 Push Cadence — Local And Remote Always In Sync

Every local commit is pushed to `origin/main` immediately. **Current remote**: `https://github.com/Attestrum/Attestrum.git`.

**Workflow per commit:** (1) Run pre-commit gates (§7). (2) `git add <specific paths>` (never `-A` or `.`). (3) `git commit -m '...'` with CHANGELOG entry staged in the same commit if release-relevant; local SESSION-LOG appended outside the commit. (4) `git push origin <current-branch>` (today: `main`).

**Why every time:** The remote is the canonical backup. CI on push:main (`ci.yml` fmt/clippy/test/cargo-deny, `determinism.yml` 4-target byte-identity matrix, `cosign-interop.yml` Sigstore round-trip) validates against GHA, not the local box — local-green ≠ CI-green. Solo workflows that batch pushes lose the GHA feedback loop and accumulate environment-drift bugs.

**Acceptable batched-push cases** (rare): a deliberate multi-commit landing sequence where commit B depends on A; a fix-forward immediately after a wrong commit.

**Never acceptable:** sitting on a green local commit indefinitely; `git push --force` to `main` without explicit approval + written recovery plan; `git commit --no-verify` (§0.5.4 anti-pattern); `git push --no-verify`.

---

## 7. Build And Test Discipline

Before EVERY commit, all of the following must pass. The `.githooks/pre-commit` hook wraps these; activate it with `git config core.hooksPath .githooks` (one-time per clone). The hook strict-blocks with no env-var bypass per §0.5.4.

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams
cargo deny check sources licenses
cargo run -p secret-scanner --release --quiet -- check
```

If any fail, fix before committing. Never disable a failing test for green light. Never `#[allow(...)]` a clippy lint without an explaining comment. Never `git commit --no-verify` to bypass the hook.

**Boundary-slippage audit (integration commits only).** When staged changes touch ≥2 crates (`git diff --staged --name-only | grep -oE '^crates/[^/]+/' | sort -u | wc -l`), run:

```bash
window=$(git log -30 --pretty=format:'%h')
for d in $(find docs/diagrams -name '*.md'); do
  lv=$(awk '/^last_verified:/ { print $2; exit }' "$d")
  case "$lv" in ''|bootstrap) continue;; esac
  pos=$(printf '%s\n' "$window" | grep -nF "$lv" | head -1 | cut -d: -f1)
  if [ -z "$pos" ]; then echo "STALE: $d (lv $lv)"
  elif [ "$pos" -ge 27 ]; then echo "WARN:  $d (pos $pos/30)"
  fi
done
```

For each WARN/STALE: re-read the diagram, confirm it still describes the post-commit code, then bump its `last_verified` to `<HEAD short-SHA> <today>` and stage it in the same commit. The §5 linter catches drift only when the diagram's named code is staged — it cannot warn about adjacent diagrams approaching staleness. E6 (`ea50d74`) is the canonical cascade this prevents.

**Gate split.** `sources` catches unapproved git pins; `licenses` catches transitive deps whose SPDX license isn't in `deny.toml`'s `allow` list. `cargo deny check bans` is omitted because `[bans].deny` is empty. `cargo deny check advisories` is CI-only (RUSTSEC index queries are slow; two transitive RUSTSEC reds currently). Historical rationale lives in `docs/research/cargo-deny-gates-rationale.md`.

**Determinism.** Byte-identical builds across Linux x86, Linux ARM, macOS, and Linux musl. Sources of non-determinism are bugs: all map iteration through `BTreeMap` or explicitly sorted `Vec`; timestamps from a single `--source-date-epoch`; no floating-point in any hash or Merkle path; `serde_json` with sorted-keys; Parquet writer pinned to `zstd` level 3, no dictionary heuristics. If a test passes locally but fails CI's determinism matrix, the test is finding a real bug.

**CI status check.** CI state changes commit-to-commit. Run `gh run list -R Attestrum/Attestrum -L 6` at session start.

---

## 8. Dependency Discipline

Do not add a crate without explicit approval. Canonical set: current workspace `Cargo.toml` lockfile + `docs/license-inventory.md`. To add: (1) surface in the plan, (2) propose name + version + license + reason, (3) wait for approval.

**Forbidden without approval:**

- No GPL or AGPL. Apache-2.0, MIT, BSD, MPL-2.0, Unlicense, CC0 only. **Approved transitive-only exception**: `Unicode-3.0` for transitive Unicode-Consortium data tables (used industry-wide via `serde_derive` → `unicode-ident`). Direct deps must be on the base list. Other transitive license not on the base list still stops the PR.
- No `unsafe` outside FFI shims to vetted C libraries. If you want `unsafe`, surface it.
- No git-pinned deps except where approved (the `hf-hub` upload-API pin is the only current one — recorded in workspace `Cargo.toml` `[patch.crates-io]` + `deny.toml` `allow-git`).
- No alpha / pre-release versions. Crates must have one stable release; pin minor versions.

**License inventory.** When you add a dependency, append a row to `docs/license-inventory.md` with name, version, SPDX ID, and date.

---

## 9. UI / Browser-Surface Changes

If a change introduces or modifies a UI surface — `verify.html`, the dataset card README rendering, or the eventual dashboard — run real-browser QA via Playwright MCP before declaring it done.

**One-time MCP setup** (per machine):

```bash
claude mcp add -s user playwright npx @playwright/mcp@latest
```

**After every UI change, run this QA:**

1. Golden path end-to-end.
2. Obvious edge cases: empty inputs, invalid inputs, missing fields, error states.
3. At least one deliberately broken input that should be rejected — confirm a useful error message.
4. Mobile viewport (375x667). Does the UI break at small width?
5. Browser console — any uncaught errors fail the QA.

**Report**: golden path pass/fail, screenshots of unexpected state, numbered bug list with one-sentence fixes, console error transcript.

Attestrum's UI surface is small but it's the user-facing trust signal. A broken verify page costs more credibility than a broken backend test.

---

## 10. Communication Style

- **Be concise.** Lead with the conclusion, then the reasoning.
- **Surface contradictions immediately.** Don't silently reconcile a conflict with this file or the build plan.
- **Push back when you have a real reason.** Not "are you sure?" but "the approach in step 3 will fail under X because of Y; suggest Z instead."
- **Don't fawn.** No "great question!", "I'd be happy to!", "absolutely!". Get to the substance.
- **Don't apologize for non-errors.** A clarifying question isn't an apology event.
- **Ask one question at a time when blocked.** Bundled questions force triage.

---

## 11. Project Scope

**Attestrum IS:**

- A deterministic Rust CLI that takes a training corpus and emits a cryptographically verifiable provenance bundle.
- Sigstore-signed, in-toto-attested, Merkle-rooted over BLAKE3.
- Open-source under Apache-2.0 OR MIT (dual-license). Copyright holder: **Hyper Beam Media LLC**. `LICENSE-APACHE` + `LICENSE-MIT` carry the canonical copyright lines; per-file SPDX headers are NOT used.

**Attestrum IS NOT:**

- A frontier-lab compliance tool.
- A registry. v1 doesn't host fingerprints or operate a hosted SaaS (only optionally federates with Rekor or Hugging Face).
- A two-sided market — the buyer is the publisher of the corpus, not the rightsholder asking "was my work used."
- An ML research project. Fingerprinting uses published algorithms (BLAKE3, ISCC, pHash, MinHash); we do not invent new ones.
- A litigation eDiscovery tool.
- A general-purpose data versioning system. Not Git for data, DVC, or lakeFS.

If asked to add something that would push scope toward an "IS NOT" item, surface the conflict before scoping.

---

## 12. Vendor Neutrality

Every artifact Attestrum emits must verify with standard public tooling — no Attestrum install required.

- **Public type URIs only.** The Sigstore bundle, in-toto Statement, Croissant JSON-LD, CycloneDX ML-BOM all use canonical public URIs. The string "Attestrum" appears only in the predicate URI prefix (`attestrum.com/`) and the informational `builderVersion` field — never in emitted format structure.
- **Domain ownership migratability.** `attestrum.com` is registered. If predicates move to a vendor-neutral namespace, the in-toto attestation framework's New Predicate Guidelines define a rename path.
- **No vendor lock-in.** Every artifact verifies with `cosign v3+ verify-blob-attestation --new-bundle-format` and no Attestrum install. The static `verify.html` works without Attestrum. The Croissant JSON-LD validates against the public schema. The Article 53 template matches the Commission's exact format.
- **Hub-publish is one target among several.** `attestrum publish` supports Hugging Face primary, GitHub Releases fallback, and static-bundle output for Zenodo or self-hosting.

When in doubt: optimize for "any user can verify this without Attestrum installed."

---

## 13. Legal & Regulatory Questions

Point to the citation, restate what the spec says, let the founder decide. Do not generate legal opinions.

---

## 14. Anti-Patterns Specifically Forbidden

- **Silent scope creep.** Don't add "while I'm in here" changes outside the approved plan. Surface; don't fix in the same commit.
- **TODO comments without owners.** Either ship it, file a tracked issue with a link, or remove the path. `// TODO: handle this later` is forbidden.
- **Dead code paths.** If a branch is unreachable, delete it. If reachable but unused, write the test. No "I'll wire this up later" stubs.
- **Eager generalization.** No trait abstraction for one concrete type "in case we need it later." Extract the trait when the second implementation appears.
- **Premature optimization.** No `unsafe`, no SIMD intrinsics, no hand-rolled allocators, no custom hash maps in v1. Safe Rust hits the 100GB-in-10-minutes target on a 16-core box.
- **Implicit network calls.** Every networking function is documented as such. Default is "no network." Network calls take an explicit client parameter and return an error type with a network variant.
- **Untested error paths.** Every `Result::Err` has at least one exercising test. Error paths existing only to satisfy the compiler are dead code.
- **"It works on my machine."** If CI determinism disagrees, CI is right.

---

## 15. When You Are Uncertain

Default response when you don't know what to do is **ask before acting**.

- Don't know which approach the founder prefers? Lay out two options with trade-offs and ask which.
- Don't know if a change is in scope? Quote the relevant sentence from this file or point at current code / diagram state and ask.
- Don't know if a dependency is approved? Cite the dep list location and ask.
- Don't know plan mode vs execution mode? Assume plan mode. Ask.
- Don't understand why a test is failing? Show the output and ask before "fixing" the test.

Asking once costs 30 seconds. Building the wrong thing costs hours.

---

*Last updated: 2026-05-28. For the per-section change history, see `git log -- CLAUDE.md`. Operational aide-memoire (formerly the Quick Reference Card) lives at `Attestrum-internal-notes/AGENT-QUICK-REFERENCE.md`.*
