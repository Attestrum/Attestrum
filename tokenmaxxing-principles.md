# Tokenmaxxing Principles v2

A workflow prompt-pack distilled from Gary Tan's *Tokenmaxxing: How Top Builders Use AI To Do The Work Of 400 Engineers* (The Light Cone, May 2026), revised for Mermaid-only diagrams and tightened across every rule.

**How to use this doc.** At the start of a feature, paste a link to this file into a Claude Code conversation, or reference it from a project's `CLAUDE.md`. It's intentionally *not* auto-loaded into every session — pull it in deliberately when you're about to build something non-trivial. "Non-trivial" means: any feature that touches more than one file, any change with conditional branching beyond a single `if`, any UI surface, any data-mutating operation, any new public API, any new persistent state, or anything you don't already know exactly how to build before opening the editor.

**Hard rule.** If you find yourself bypassing one of these principles "just this once," stop. Either commit to the bypass in writing (a one-line note in `CHANGELOG.md` explaining why this case doesn't need the principle) or apply the principle. No silent skips.

---

## Principle 1 — Mermaid Diagrams Before Code

**Rule.** Before writing any implementation code for a non-trivial feature, produce Mermaid diagrams covering every dimension that applies. Cover all that fit; don't pad with diagrams that don't add information.

- **Data flow** — flowchart, source to sink, with every transformation node labeled.
- **State machines** — `stateDiagram-v2`, every state and every transition that can fire.
- **Dependency graph** — flowchart, modules and their imports, including any external services.
- **Processing pipeline** — flowchart, stages with concurrency boundaries marked.
- **Decision tree** — flowchart, every conditional branch in the request lifecycle.
- **User flows** — `sequenceDiagram`, user → UI → backend → response, including failure modes.
- **Error and edge paths** — flowchart or `stateDiagram-v2`, what happens when each thing breaks.

**Diagram type rules.** Use the right type or the diagram fails its job:

| Situation | Required Mermaid type |
|---|---|
| Pipelines, dependency graphs, decision trees without internal state | `flowchart` |
| Lifecycles, state machines, anything with discrete states + transitions | `stateDiagram-v2` |
| Multi-actor flows, network calls, API conversations, auth flows | `sequenceDiagram` |
| Stable public APIs, class structures, type relationships | `classDiagram` |
| On-disk schemas, database tables, persistent data structures | `erDiagram` |

**Where the diagrams live.** Inside the project being worked on, at `docs/diagrams/<feature-name>/<diagram-type>.md`. One Mermaid block per file. Every file carries YAML frontmatter:

```yaml
---
title: "Short imperative title"
models: "<paths to the code or specs this diagram describes>"
source_of_truth: code      # one of: code | diagram | spec
last_verified: <commit SHA short> <YYYY-MM-DD>
diagram_type: flowchart    # or stateDiagram-v2, sequenceDiagram, classDiagram, erDiagram
---
```

- `source_of_truth: code` — the code is authoritative. The diagram must be re-verified whenever the code changes.
- `source_of_truth: diagram` — the diagram is authoritative. Used when the diagram is the contract that code must implement. Flips to `code` once the implementation exists.
- `source_of_truth: spec` — an external specification is authoritative (an RFC, a vendor API contract, a regulation). Drift means the implementation is wrong, not the spec.

**Why.** Forces context into latent space *and* into the human reviewer's eye before code commits the team to a structure. Dramatically reduces half-built features, "looked right, broke on first user" bugs, and the worst class of error: the one where the implementation diverged from the mental model and nobody noticed for three weeks.

**Trigger phrasing** (copy/paste ready):

> Before you write any code, produce Mermaid diagrams for this feature. Cover every dimension that applies: data flow, state machines, dependency graph, processing pipeline, decision tree, user flows, error paths. Each diagram in its own file under `docs/diagrams/<feature-name>/`, with the four-field YAML frontmatter. Use the right Mermaid type per the table in Tokenmaxxing Principles §1. We will review the diagrams before any code is written.

---

## Principle 2 — Mermaid Renders Inline; No Image Pipeline

**Rule.** Mermaid is the rendering. Don't generate PNGs, don't generate SVGs, don't run an image-generation API, don't pre-render anything. GitHub renders Mermaid natively. VS Code renders it natively. Obsidian renders it natively. Claude renders it natively in chat. Every consumer of the diagram already has a rendering path that produces a textually-diffable, version-controlled, light-and-dark-theme-correct image at view time.

**Why this is a hardening, not a regression from the prior version.**

- **Diffability.** Mermaid is text. Git shows real diffs. Image renders break diffability completely.
- **Determinism.** The same Mermaid produces the same render every time. AI-generated raster images don't.
- **Cost.** Zero. The prior version charged per render against an OpenAI API key.
- **Latency.** Zero render call. The prior version waited on a network round-trip.
- **Faithfulness.** Mermaid renders match the source exactly. AI raster renders interpret the prompt and drift — they were "scene-style raster images, not faithful character art," which is a polite way of saying they got the diagram wrong sometimes.
- **Maintenance.** A wrapper script, an API key, an environment variable, a cost ledger, exponential backoff retries, and a model name to keep current. All gone.

**Hard prohibition.** No project under this principles file ships with a `gen-image-*.sh`, a `render-diagrams/` directory, or any commit that adds a PNG or SVG to source control for the purpose of "rendering a diagram." Diagrams are Mermaid in Markdown, period.

**Permitted exceptions** (narrow, two cases only):

1. **Architecture diagrams in marketing material.** A landing page hero diagram is allowed to be a hand-designed SVG, because it's marketing not engineering. It does not live under `docs/diagrams/`.
2. **External-format requirements.** A specific regulator or standards body requires PDF/PNG output (rare). In that case the Mermaid is still the source of truth and a CI job exports the regulator's format from the Mermaid via `mmdc` (`@mermaid-js/mermaid-cli`), pinned by SHA. The exported artifact is gitignored and regenerated on demand.

**Validation in CI.** Every Mermaid block in every Markdown file in the repo is parse-checked on every PR:

```bash
npm install -g @mermaid-js/mermaid-cli@10.9.1
find . -name '*.md' -print0 | xargs -0 -I {} sh -c \
  'awk "/^\`\`\`mermaid/,/^\`\`\`/" "{}" | mmdc --input - --output /dev/null'
```

A parse failure fails the build. A diagram that references a Rust type, a function, an API endpoint, or a file path that doesn't exist in the codebase fails the build (use a project-specific linter for this — examples in §7).

---

## Principle 3 — 10-Star Experience Review (CEO Plan)

**Rule.** After Claude produces *any* implementation plan, run it through the Brian Chesky ladder before approval. Three short prompts, massive unlock. The first prompt expands imagination; the second filters for shippability; the third — new in v2 — filters for *the right ambition vector for this product*.

**Trigger phrasing.**

1. > What would a 6-star version of this plan look like? An 8-star version? A 10-star version? Describe what's different at each level — don't just list features.

2. > Of the 10-star moves you described, which deliver 10x more value for only 2x the effort? Rank them, and tell me which ones you'd cut from the plan and which you'd add.

3. > For each of the 10-star moves you'd add, name the user it's for. If the user is "everyone" or "a future enterprise customer," the move probably doesn't belong in this plan. Cut it.

**Why three prompts and not two.** The original two-prompt version surfaced ambitious moves but didn't filter for product fit. The third prompt is a fence against scope creep dressed as ambition. A 10-star idea aimed at a user you don't have yet is a 0-star idea for this release.

**When to run it.** After every initial plan. Before approval. Before diagrams (so the diagrams reflect the chosen ambition level, not the first-pass plan).

**Anti-pattern this fences off.** Claude producing a sensible plan, getting CEO-laddered into an aggressive plan, then producing diagrams for the aggressive plan, then spending three sprints building toward a user that doesn't exist. The third prompt forces grounding back to the actual customer before the diagrams lock in the work.

---

## Principle 4 — Plan-Mode Queue Workflow

This is the canonical loop. Claude-only. The original Gary Tan version included a `/codex` cross-check; intentionally omitted here because this user runs Claude exclusively and the cross-check adds latency without proportional benefit on the kind of work shipped from this machine.

**The loop, per feature.**

1. **Enter plan mode.** No code yet. No file edits yet. Set the expectation explicitly: "We are in plan mode. Do not run commands or edit files until I say go."
2. **Have Claude propose the plan.** Numbered commits in execution order, each commit small enough to review in one sitting. Each commit references the relevant principle or spec.
3. **Apply Principle 3** — the three-prompt CEO ladder. Revise the plan based on the answers.
4. **Apply Principle 1** — Mermaid diagrams in `docs/diagrams/<feature-name>/`. Every dimension that applies, with frontmatter.
5. **Eng review pass.** Ask Claude to critique its own plan and diagrams against the trigger below. Revise.
6. **Diagram-code consistency promise.** Claude commits, in writing in the plan, that every diagram referencing a type, file, or endpoint matches what will be built — and that drift will be fixed in the same commit as the code change.
7. **Approve and exit plan mode.** Give Claude explicit "go." Until you do, it doesn't run a single command.
8. **Implement.** One commit at a time. After each commit, append a session entry to `CHANGELOG.md` and `SESSION-LOG.md` (format below).
9. **Apply Principle 6** — real-browser QA before declaring the feature done.

**Eng review trigger phrasing.**

> Before we approve, do an engineering review of your own plan and diagrams. Find:
> 1. Bugs — anything that won't work as drawn.
> 2. Edge cases the diagrams don't cover.
> 3. Missing tests — what state transitions or branches lack test coverage in the plan.
> 4. Integration risks — anything that could break a piece of the codebase outside the feature scope.
> 5. User-surprising behavior — anything a reasonable user would not predict from the UI alone.
> 6. Diagram-vs-plan drift — anywhere the diagram and the written plan disagree.
> Revise both the plan and the affected diagrams to address what you find. Note in the revision what you changed and why.

**Session entry format.** Append to BOTH `CHANGELOG.md` and `SESSION-LOG.md` at every commit:

```markdown
## [YYYY-MM-DD] — <task or commit summary>
- **Files changed**: <paths>
- **Diagrams touched**: <paths or "none">
- **Summary**: <one paragraph>
- **Findings**: <surprises, decisions made, deferred work>
- **Open questions**: <anything you want the next session to address>
- **Tokens used**: <approx, if known>
```

**Batching.** Queue multiple plans this way before executing any of them. Run them as a batch when ready, then run Principle 6 on each in turn. The bottleneck on a one-person shop used to be manual QA. Principle 6 collapses that into a tight agent-driven loop where you only eyeball the screenshots.

**Plan-mode discipline (the rule that makes the rest work).** When in plan mode, the agent does not create files outside `docs/diagrams/<feature-name>/`, does not run cargo / npm / pip / git commands, and does not edit source files. The plan mode → execution mode transition is gated by an explicit "go" from the human. No exceptions. If Claude says "I'll just scaffold a quick file to test this idea," that's a discipline failure — push back and reset.

---

## Principle 5 — Thin Harness, Fat Skills

The mental model that ties everything else together.

**Harness** = the user-input → LLM → tool-call loop. **Don't build one.** Claude Code is the harness. Cursor is a harness. Aider is a harness. Reaching for "let me wrap this in a custom agent" is almost always the wrong move. The only legitimate reasons to build a custom harness are:

1. You're running fully unattended (no human in the loop at all), in which case you need orchestration the chat harnesses don't provide.
2. You're integrating LLM calls into a production user-facing product (in which case it's not a harness, it's a feature).
3. You're shipping infrastructure others will harness against (in which case it's an SDK or a server, not a harness).

If none of those three apply, use the existing harness.

**Markdown** = where judgment, special cases, and "how a thoughtful human would handle this" live. Skills, prompts, principles docs like this one, project READMEs, CLAUDE.md files, design briefs, runbooks. **Markdown is code** — it just compiles through latent space instead of a CPU. The hardening from v1 is acknowledging that good markdown is engineered: it has structure, it has examples, it has anti-examples, it has frontmatter, it gets versioned, it gets reviewed, it has tests (the tests are "did the agent do what the markdown said when invoked").

**Code** = reserved for *deterministic* actions: database queries, HTTP calls, file I/O, cryptographic operations, computation, parsing, anything where there's exactly one right answer and no judgment involved. The signature of "this should be code" is that two competent humans handed the same inputs would write nearly identical output. The signature of "this should be markdown" is that two competent humans handed the same inputs would write *different* output that both achieve the goal.

**Heuristic for "code or markdown?"** If a competent human would handle the case with judgment — weighing context, handling unstated edge cases, reading intent, accommodating a tone — it belongs in markdown. If they'd just execute a deterministic operation with no thought, it belongs in code. Most "this code is so brittle" pain comes from putting markdown work into code. Most "this prompt is so vague" pain comes from putting code work into markdown.

**A v2 hardening.** Don't write markdown that the agent will have to re-derive. If the markdown says "use the right tone for the user's industry," that's a re-derivation. Better: "use the tone from `docs/tone-examples/<industry>.md`," and ship the tone examples. Markdown that defers all judgment back to the agent is just a wishlist.

---

## Principle 6 — Real-Browser QA Before Declaring Done

**Rule.** Before any UI-touching feature is "done," the agent verifies it in a real Chromium browser via Playwright MCP — golden path plus the obvious edge cases plus at least one deliberately-broken-input case. Manual eyeball QA by the human is the *last* check, not the first.

**Why.** This is the bottleneck Gary hit head-on in the transcript — features that had passed unit, integration, and end-to-end tests but still required the human to "pop open the Rails server, load that user, make it into that configuration, and manually just make sure it works." Claude-in-Chrome MCP at 2–3 seconds per turn was too slow for QA. The fix is wrapping Microsoft's Playwright MCP in a persistent session. A *persistent* browser session is fast enough for an agent to drive; cold-start browser calls are not.

**One-time setup** (registers Microsoft's official Playwright MCP at user scope, available in every project):

```bash
claude mcp add -s user playwright npx @playwright/mcp@latest
```

The `-s user` flag is critical — without it the MCP only registers for the current project's directory and you'll re-register it dozens of times.

The MCP stays alive across calls — that's the persistence trick that makes it fast. Defaults to a headed (visible) browser; pass `--headless` in the args if you want it invisible for batch runs.

**Trigger phrasing** (after implementation, before reporting "done"):

> Use the Playwright MCP to verify this feature in Chromium. Test:
> 1. The golden path end-to-end.
> 2. The obvious edge cases: empty inputs, invalid inputs, missing required fields, error states, slow network if relevant.
> 3. At least one deliberately broken input that should be rejected — confirm it is rejected with a useful error message.
> 4. The mobile viewport (375x667). Does the UI break at small width?
> 5. Browser console errors. Read the console; any uncaught errors fail the QA.
> Report back with: (a) golden path pass/fail, (b) screenshot of any unexpected state, (c) a numbered list of bugs found with one-sentence fixes, (d) console error transcript. Don't stop at "the page loaded."

**When it fires.**

- Any feature that surfaces in the UI (new page, component, form, data display, modal, toast, animation).
- Any data mutation the user sees the result of (create / update / delete from a UI action).
- Any change to a deployed site (Netlify, Vercel, Cloudflare Pages, any custom host) before flipping live.
- Any auth-flow change. Auth is the highest-cost regression class because users hit it first.
- Any feature behind a feature flag, on both the on and off states.

**When it doesn't.**

- Pure backend changes verifiable by `curl` or a SQL query — don't waste a browser launch.
- CLI tools, scripts, cron jobs — verify with the tool's own invocation.
- Documentation, config, refactors with no UI surface change.

**Output expectation.** The agent reports back with (a) whether the golden path worked, (b) screenshots of any unexpected state, (c) a numbered list of bugs found, (d) a suggested fix per bug, (e) the browser console error transcript. *Then* you eyeball the screenshots and decide what's a real issue vs. a false alarm.

**A v2 hardening.** Add the broken-input test, the mobile viewport check, and the console-error read. The original version checked the golden path and obvious edges. The new additions catch: rejection-path bugs (where the system silently accepts garbage), mobile-layout bugs (where the desktop looked fine), and JavaScript runtime errors that didn't break the visible flow but broke something else on the page.

---

## Principle 7 — CI Enforces What Markdown Promises

**New in v2.** The first six principles are worthless if the team (or the agent) silently stops following them. CI exists to make stopping impossible.

**Rule.** Every project that adopts these principles ships a `.github/workflows/principles.yml` (or equivalent) that fails any PR violating principles 1, 2, or 6.

**What CI checks.**

1. **Every Mermaid block parses.** `mmdc --input - --output /dev/null` on every fenced ```mermaid block under `docs/`. Parse failure fails the build.
2. **Frontmatter is present.** Every `docs/diagrams/**/*.md` has the four required keys (`title`, `models`, `source_of_truth`, `last_verified`). Missing key fails the build.
3. **`last_verified` is fresh.** The commit SHA in `last_verified` must be within the last 30 commits OR within the current PR's commit range. Stale SHA fails the build — drift between code and diagram caught early.
4. **Forward references resolve.** Every type, function, file, or endpoint named as a node label in a Mermaid diagram exists in the codebase. Dangling reference fails the build.
5. **Reverse references resolve.** Every `pub` item in the codebase (or every exported function in a TypeScript project) is referenced by at least one diagram. Missing reverse reference fails the build for new code; existing untouched code is grandfathered.
6. **No raster diagrams.** Any commit adding a `.png` or `.svg` under `docs/diagrams/` fails the build. Mermaid-only.
7. **Playwright QA evidence for UI changes.** Any PR touching files under a configured UI path (e.g. `src/components/`, `app/`, `pages/`) requires a Playwright QA report attached as a commit comment or a CI artifact. No report, no merge.

**Implementation note.** A simple Rust or Node binary in `tools/principles-linter/` is enough. ~300 lines. Run locally with `principles-linter check` and in CI as a workflow job. The exact implementation is per-project, but the seven checks above are the contract.

**Why this principle exists.** Markdown promises are aspirational without enforcement. Every founder I have ever seen adopt "diagrams first" verbally has, within four weeks, silently dropped it under deadline pressure. CI is the only thing that holds. Make it hold.

---

## Quick Reference

| # | Principle | Trigger | When it fires |
|---|-----------|---------|---------------|
| 1 | Mermaid diagrams first | "Before you write any code, produce Mermaid diagrams for…" | Start of any non-trivial feature |
| 2 | Mermaid renders inline; no image pipeline | Native rendering in GitHub / VS Code / Obsidian / Claude | Automatically — no action needed |
| 3 | 10-star CEO review (three prompts) | "6/8/10-star version?" + "10x value for 2x effort?" + "Name the user for each addition" | After every initial plan |
| 4 | Plan-mode queue workflow | plan mode → CEO review → diagrams → eng review → approve → implement → QA | Per feature; batch multiple before executing |
| 5 | Thin harness, fat skills | Mental model — applies to every architecture decision | Any time you're tempted to build a custom harness, write markdown that defers all judgment, or stuff judgment into code |
| 6 | Real-browser QA before done | "Use the Playwright MCP to verify this feature in Chromium…" | After implementing any UI-touching feature, before declaring done |
| 7 | CI enforces what markdown promises | `.github/workflows/principles.yml` runs `principles-linter check` | Every PR, automatically |

---

## Changelog From V1

- **Principle 1** — Renamed from "ASCII diagrams" to "Mermaid diagrams." ASCII was always a stepping stone to a real diagram; Mermaid is the real diagram. Added the diagram-type table (which Mermaid type for which situation), the frontmatter requirement (four-field YAML), and the `source_of_truth` field with three valid values.
- **Principle 2** — Completely rewritten. v1 mandated a `gpt-image-2` render of every ASCII diagram. v2 forbids it — Mermaid renders natively in every consumer (GitHub, VS Code, Obsidian, Claude chat). Cost goes to zero, faithfulness goes to perfect, diffability is restored, and the wrapper script + API key + env vars + cost ledger all disappear.
- **Principle 3** — Added a third prompt ("Name the user for each addition"). The two-prompt version surfaced ambition but didn't filter for product fit; the third prompt fences off scope creep dressed as ambition.
- **Principle 4** — Tightened plan-mode discipline. Explicit "the agent does not create files outside `docs/diagrams/<feature-name>/` in plan mode" rule. Added the session-entry format. Added the diagram-code consistency promise as step 6.
- **Principle 5** — Added the three legitimate reasons to build a custom harness, the markdown-isn't-a-wishlist hardening, and the heuristic test for "two competent humans" deterministic-vs-judgment split.
- **Principle 6** — Added the broken-input test, the mobile viewport check, and the browser console-error read. The original version caught visible bugs; the new additions catch silent-acceptance bugs, layout regressions, and runtime errors.
- **Principle 7** — New. The first six principles are aspirational without CI enforcement. Seven specifies the seven checks every project ships in `principles-linter`.
- **General** — Added the "non-trivial" definition up top, the hard rule against silent skips, and the diagram-type selection table. The doc is longer because the rules are tighter.

---

*Source: Derived from* Tokenmaxxing: How Top Builders Use AI To Do The Work Of 400 Engineers *(The Light Cone, May 2026). v1 of this document used ASCII diagrams plus a `gpt-image-2` render pipeline; v2 replaces both with native Mermaid and adds Principle 7 (CI enforcement). Full transcript referenced in v1:* `Tokenmaxxing_ How Top Builders Use AI To Do The Work Of 400 Engineers_128k.txt`.
