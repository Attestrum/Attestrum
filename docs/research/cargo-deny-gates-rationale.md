# Gating a Provenance Tool's Own Supply Chain

### Where each dependency check belongs — and why a tool that vouches for data must first vouch for itself — a methodology report from the Attestrum project

**Status:** research / methodology. Describes how Attestrum governs its own dependency graph with `cargo-deny`, why each of the four checks runs where it runs (local pre-commit, CI, or both), and why supply-chain discipline is load-bearing for a tool whose entire value proposition is verifiable provenance. Derived from the repository's `deny.toml`, the `.githooks/pre-commit` hook, the `audit` job in `.github/workflows/ci.yml`, and `CLAUDE.md` §7–§8. Descriptive and prescriptive: it explains the placement rule and how a reader can adopt it. All configuration values are quoted from the committed `deny.toml` as of this writing; where the live config has drifted from older notes, this document reflects the config, not the notes.

This file supersedes the short historical rationale of the same name (migrated out of `CLAUDE.md` §7 on 2026-05-27 during a character-count audit). The operative rule — `cargo deny check sources licenses` as gate 5 of the six-gate pre-commit ritual — still lives in `CLAUDE.md` §7; this document is the reasoning behind it.

---

## Abstract

A tool that issues cryptographic assurances about *someone else's* data inherits an obligation it cannot wave away: its own build must be at least as trustworthy as the claims it emits. A provenance bundle signed by a binary that was itself assembled from unvetted, unknown-licensed, or quietly-vulnerable dependencies is an assurance resting on an unexamined foundation. This report describes how **Attestrum** — a deterministic Rust CLI that compiles training corpora into verifiable provenance bundles — governs its own dependency graph using `cargo-deny`, and it argues two theses. The first is a *placement* thesis: the right question for any dependency gate is not only *whether* to run it but *where*, and the deciding axis is whether the check is fast, offline, and **deterministic** (→ belongs in the local pre-commit gate, where it stops a regression before it costs a push) or slow, network-bound, and **time-varying** (→ belongs in CI, where a green result yesterday is not a promise about today). Attestrum's split — `sources` and `licenses` enforced locally *and* in CI, `advisories` and `bans` in CI only — is the worked example. The second is a *trust-inheritance* thesis: license hygiene, git-pin governance, and documented advisory triage are not bureaucratic overhead but direct underwriters of the two product promises Attestrum makes elsewhere — **byte-determinism** and **vendor neutrality**. We give the four checks, the placement rule, the transitive-only license-exception pattern, the triple-entry git-pin discipline, the practice of accepting an advisory against a *stated threat model* rather than blind-ignoring it, and an explicit account of what these gates do *not* establish.

---

## 1. Two questions about a dependency gate

Every project that pulls in third-party code faces the same family of risks: a dependency under a license incompatible with the project's own; a dependency fetched from a source nobody approved; a dependency with a known vulnerability; a dependency that pulls in three copies of the same crate at three versions. The Rust ecosystem's standard answer is `cargo-deny`, which bundles four checks — `advisories`, `bans`, `licenses`, `sources` — behind one command.

The naïve adoption is to run `cargo deny check` everywhere and call it done. That is not wrong, but it leaves two more interesting questions unanswered, and Attestrum's configuration is an answer to both.

**The first question is placement.** A check that runs in the local pre-commit hook stops a bad commit before it is ever made — no wasted push, no red CI run, no context-switch back to a change the author has mentally closed. But the local hook runs on every commit, so a slow check taxes every commit, and a check whose result depends on the network or on a remote database that changes hour to hour cannot give a stable, reproducible answer at commit time. So *where* a check belongs is a real engineering decision, not a default. §3 gives the rule Attestrum uses.

**The second question is why a provenance tool should care more than most.** Attestrum's product is trust: it takes a corpus and emits a bundle that a third party can verify *without trusting Attestrum* — with stock `cosign` and no Attestrum install (the vendor-neutrality promise; see `provenance-without-disclosure.md` and `CLAUDE.md` §12). But the *binary that produces the bundle* is assembled from a dependency graph, and nothing about a clean signature says anything about whether that graph was governed. A tool that asks the world to take its outputs on cryptographic faith should not ask the world to take its *inputs* on blind faith. §7 develops this.

---

## 2. The four checks, briefly

`cargo-deny` runs four independent checks; Attestrum's `deny.toml` configures all four, and CI runs all four (§3). What each catches:

- **`licenses`** — every crate in the resolved graph must carry an SPDX license expression on an allow-list. A new transitive dependency under, say, GPL-3.0 fails the build. Attestrum's allow-list is a strict permissive set (`CLAUDE.md` §8); §4 covers it.
- **`sources`** — every crate must come from an approved *source*: the standard crates.io registry, or a git URL on an explicit `allow-git` list. A `[patch.crates-io]` override or git-pinned dependency from an unlisted URL fails. §5 covers it.
- **`advisories`** — every crate is checked against the [RUSTSEC](https://rustsec.org/) advisory database for known vulnerabilities, unsoundness, unmaintained status, and yanked releases. §6 covers it, including why it is the one check that *cannot* be deterministic.
- **`bans`** — the project's own deny-list of specific crates or versions, plus structural policy: wildcard version requirements, and multiple versions of the same crate in one graph. §3.3 covers why Attestrum runs it but only in CI.

The checks are orthogonal. A graph can pass `licenses` and fail `advisories`, or pass `sources` and fail `bans`. That orthogonality is what makes the placement question (§3) worth asking per-check rather than once for the whole command.

---

## 3. The placement principle

### 3.1 The rule

> **A dependency check belongs in the local pre-commit gate when it is fast, offline, and deterministic. It belongs in CI when it is slow, network-bound, or time-varying. A check that is both fast/offline/deterministic *and* important enough to never regress belongs in both.**

The local hook's job is to stop a regression at the cheapest possible moment — before the commit exists. That job is only worth doing for a check that can give a fast, stable, reproducible answer without reaching the network, because the hook runs on *every* commit and must not depend on conditions outside the working tree. CI's job is the authoritative, environment-controlled verdict — the place where a slow or network-bound check can run once per push without taxing the inner loop, and where the result is recorded against a known toolchain.

### 3.2 How Attestrum splits the four checks

| Check | Local pre-commit | CI | Why |
|---|---|---|---|
| `licenses` | ✅ | ✅ | Fast, offline, deterministic. A new bad-licensed dep is caught at commit time; CI re-asserts it under a controlled toolchain. |
| `sources` | ✅ | ✅ | Fast, offline, deterministic. An unapproved git pin is caught at commit time. |
| `advisories` | ❌ | ✅ | Slow (queries the RUSTSEC index) and **time-varying** — a clean graph today can carry a new advisory tomorrow with no code change. Cannot be a stable local gate. |
| `bans` | ❌ | ✅ | Currently low-yield locally (the explicit deny-list is empty), but its structural policy — `wildcards = "deny"` — is real and runs in CI. Kept off the local hook to keep the inner loop lean. |

Concretely, the local hook (`.githooks/pre-commit`, gate 5) runs:

```bash
cargo deny check sources licenses
```

and CI's `audit` job (`.github/workflows/ci.yml`) runs the full set via the upstream action:

```yaml
- name: Run cargo-deny
  uses: EmbarkStudios/cargo-deny-action@v2
  with:
    command: check
    arguments: --all-features    # all four checks
```

`cargo deny check --all-features` with no check names runs `advisories`, `bans`, `licenses`, and `sources` together. So the local hook is a fast, deterministic *subset* of the authoritative CI run, chosen to catch the two regressions an author can actually introduce in a single commit — a bad license or an unapproved source — at the moment they introduce it.

### 3.3 A note on `bans`

Older notes described `bans` as "omitted because `[bans].deny` is empty," which undersells it. The explicit per-crate deny-list *is* empty (`deny = []`), and the multiple-versions policy is set to `warn`, not `deny` — so on those two axes the check is currently low-yield. But `[bans]` also sets `wildcards = "deny"`, which is an active, enforced policy: a dependency declared with a wildcard version requirement (`"*"`) fails CI. So `bans` is not a no-op; it is *omitted from the local hook* (to keep the inner loop fast) while still doing real structural work in CI's `--all-features` run. The empty `deny` list is a deliberate, reversible default — the `deny.toml` comment notes it exists as the place to block a known-bad version *before* RUSTSEC catches it, should that ever be needed.

### 3.4 Why "both" is not redundant

Running `licenses` and `sources` in *both* the local hook and CI is not belt-and-suspenders for its own sake. The local hook can be bypassed (any contributor can edit it; the hook intentionally has no env-var override but is still just a file), and a contributor who has not run the one-time `git config core.hooksPath .githooks` setup has no hook at all. CI is the gate that cannot be skipped on the path to `main`. The local copy is an *early-warning optimization* layered on top of the authoritative CI check — it makes the common case fast, but CI is the line of record.

---

## 4. License discipline and the transitive-only exception pattern

### 4.1 The allow-list is strict by policy

`CLAUDE.md` §8 fixes the base license policy: no GPL or AGPL; direct dependencies must be Apache-2.0, MIT, BSD, MPL-2.0, Unlicense, or CC0. `deny.toml`'s `[licenses].allow` array is the machine-enforced form of that policy. A crate whose SPDX expression is not on the list fails the build, and `confidence-threshold = 0.8` requires `cargo-deny` to be reasonably sure of the license it detected before accepting it.

### 4.2 The pattern: direct deps stay narrow, transitive deps get documented exceptions

The interesting structure is not the base list but how it *grows*. A permissive-licensed direct dependency occasionally drags in a transitive crate under a license that is equally permissive but not on the base list. The policy is **not** to widen the base list (which would loosen what a *direct* dependency may be), but to add a narrowly-scoped, individually-commented exception for the specific transitive license, with the comment recording what the license is, which crate needs it, and why it is trust-equivalent to the base set.

The allow-list's own comments record this history. Each addition is a transitive-only exception:

| License | Added for (transitive path) | Trust rationale (per the `deny.toml` comment) |
|---|---|---|
| `Unicode-3.0` | `unicode-ident` (via `serde_derive`) | Unicode Consortium data-table license; OSI-approved, permits commercial use, attribution-only. |
| `ISC`, `MIT-0` | `aws-lc-rs` / `aws-lc-sys` (via `sigstore` → `reqwest` → `rustls`) | ISC ≈ BSD-2-Clause; MIT-0 is MIT *minus* the attribution clause — strictly more permissive than allow-listed MIT. |
| `Zlib` | `foldhash` (via `hashbrown` v0.15+) | Permissive; BSD-2-Clause plus a "do not misrepresent origin" clause. |
| `CDLA-Permissive-2.0` | `webpki-root-certs` (the Mozilla root-CA bundle) | Linux Foundation data-license; permits use/modification/redistribution, attribution-only; standard for distributed data sets. |
| `NCSA` | `libfuzzer-sys` (via `image` → `ravif` → `rav1e`) | University of Illinois/NCSA license; BSD-3-Clause-equivalent in effect. |
| `BSL-1.0` | `xxhash-rust` (via `iscc-lib` for ISO 24138 CDC chunking) | Boost Software License; strictly more permissive than MIT (no attribution for binary redistribution). |

The discipline this encodes: **a transitive license you have not personally read is a blocker, not a warning.** Each row above was a build break the first time the dependency appeared, resolved by a human reading the actual license, confirming it was trust-equivalent to the permissive base set, and committing the exception with the reasoning attached. The cost is a moment's friction the first time a new transitive license appears; the payoff is that the project never silently ships a dependency under a license nobody examined — which, for a tool meant to produce auditable artifacts, is exactly the failure it cannot afford (§7).

This is why the `licenses` check earns its place in the *local* hook: the friction is most useful at commit time, when the author who just added `image` or `iscc-lib` still has the context to triage the new transitive license, rather than after a wasted push when the change has already been mentally closed.

---

## 5. Source discipline: git-pin governance

### 5.1 What `sources` enforces

`[sources]` sets `unknown-registry = "deny"` and `unknown-git = "deny"`: every crate must resolve from `allow-registry` (the standard crates.io index) or from a git URL on the explicit `allow-git` list. The effect is that a contributor cannot quietly point a dependency at an arbitrary fork or branch — a `[patch.crates-io]` override or a git dependency from an unlisted URL fails the build.

### 5.2 The triple-entry rule

A git pin is allowed only when it appears in three places at once, and the `sources` check is what enforces the first leg of that triple:

1. `deny.toml`'s `allow-git` — so the source check passes.
2. Workspace `Cargo.toml`'s `[patch.crates-io]` — the actual override that redirects the dependency.
3. A row in `docs/license-inventory.md` — the human-readable record of *why* the pin exists and when to drop it.

Each leg has a different job: (1) is the machine gate, (2) is the mechanism, (3) is the institutional memory. A pin missing from (1) fails CI; a pin missing from (3) is an undocumented liability waiting to be forgotten. `CLAUDE.md` §8 requires founder approval before any pin is added to this triple.

### 5.3 The two current pins

The `allow-git` list carries exactly two entries, each with a comment recording its origin and its exit condition:

- **`Attestrum/sigstore-rs`** — a project-owned fork carrying an `email: Option<String>` patch that lets `sigstore-rs`'s `IdentityToken::try_from` accept workload-identity OIDC tokens (GitHub Actions, GitLab CI). This is the patch that makes Attestrum's keyless CI signing path possible. Exit condition: drop it once the upstream PR merges and a `sigstore-rs` release past 0.14.0 carries the fix.
- **`huggingface/hf-hub`** — pinned to `master` because the upload API (`HFRepository::upload_file` / commit-creation flow) needed by the publish path has not yet landed in a crates.io release. Exit condition: drop it once that API ships in a release.

The discipline here mirrors §4's: a git pin is a temporary, *documented* deviation with a named owner and a written exit condition — never a silent redirect. Both pins are exactly the kind of "upstream-dependency posture change" that `CLAUDE.md` flags for explicit approval, and both are recorded as such.

---

## 6. Advisory triage: the one check that cannot be deterministic

### 6.1 Why `advisories` is CI-only

The `advisories` check is the clearest case for the §3 placement rule. It is *time-varying*: it compares the graph against the RUSTSEC database, which is updated continuously by people who are not on this project. A commit that passes the advisory check today can fail it tomorrow with **no change to a single byte of the code or the lockfile** — because someone published a new advisory overnight. A gate whose verdict can flip without any local change cannot be a stable pre-commit gate; it would block commits over conditions the author did not create and cannot fix in that commit. It also queries a remote index, so it is slow and network-bound. Both properties point the same way: CI, not the local hook.

This is the precise inverse of `licenses`/`sources`, whose verdict is a pure function of the working tree and is therefore safe to assert at commit time.

### 6.2 Accepting an advisory against a stated threat model

When a transitive dependency does carry an advisory and there is no upstream fix to pull, the choices are: vendor-fork the crate, rip out the feature that pulls it in, or *accept the advisory on documented grounds*. `cargo-deny` supports the third via `[advisories].ignore`, which takes a list of RUSTSEC IDs. The discipline Attestrum applies is that an ignore entry is not "make the red go away" — it is a **written threat-model argument** for why the advisory does not apply to *this* project's actual usage, founder-approved before it lands.

`deny.toml` currently ignores two IDs, each with a multi-paragraph justification:

- **`RUSTSEC-2024-0436`** (`paste` marked unmaintained). The argument: `paste` is a purely syntactic proc-macro buried deep in transitive trees (arrow / parquet / sigstore downstream). A proc-macro runs *only at compile time* — there is no runtime code path in the deployed binary, so "unmaintained" carries no runtime security or correctness risk. An actual CVE in `paste` would be a *different* RUSTSEC ID and would fire independently, because `[advisories].ignore` is ID-scoped, not crate-scoped — so the ignore does not blind the project to a future real vulnerability in the same crate.
- **`RUSTSEC-2023-0071`** (Marvin Attack — an RSA timing side-channel in the `rsa` crate, reached via `sigstore` → `rustls` → `rsa`). The argument: Attestrum's signing flow uses *ephemeral* keys issued by Fulcio per signing (the Sigstore keyless flow); it holds no long-lived RSA private keys. The transitive `rsa` crate is reached only for *verifying* peer TLS certificates (Fulcio, Rekor), which is public-key verification — and the Marvin Attack is a decryption-timing attack against a private-key holder, which does not apply to the verify side. Practical attack surface in Attestrum's flow: near-zero.

The shape of both arguments is the same and is the point: an ignored advisory is accompanied by a specific reason it does not reach this project's threat model, scoped to the single advisory ID, with a maintained pointer to the upstream fix's status. The result is that CI's `advisories` check is **green** — not because the advisories were silenced, but because each was triaged to a documented, ID-scoped acceptance. (Earlier project notes described advisories as "red on two carry-forward RUSTSEC items"; that described the state *before* the triage landed. The committed `deny.toml` is the current truth.)

### 6.3 What the ID-scoping buys

Ignoring `RUSTSEC-2024-0436` does *not* ignore `paste`; it ignores *that one advisory about `paste`*. If a genuine vulnerability in `paste` were published as `RUSTSEC-20XX-NNNN`, the advisory check would fail on the new ID and force a fresh triage. This is the difference between "we decided this specific known issue does not apply to us" and "we stopped looking at this crate" — the former is defensible, the latter is how supply-chain incidents happen.

---

## 7. Why this underwrites the product

The placement and discipline above would be reasonable hygiene for any project. For Attestrum they are more than hygiene, because of what Attestrum *is*: a tool that asks third parties to trust its outputs cryptographically, without trusting the tool itself. That promise has a quiet precondition — the tool's *own* construction has to be at least as governed as the assurances it emits. Two of the project's headline properties depend directly on the dependency gates.

**Vendor neutrality (`CLAUDE.md` §12).** The product promise is that every emitted artifact verifies with stock public tooling and no Attestrum install. A dependency under a copyleft or unusual license, or one fetched from an unvetted source, is a contamination risk to that promise: it can constrain how the tool may be distributed or relied upon, or introduce a component whose provenance the project itself cannot speak to. The `licenses` and `sources` gates are what keep Attestrum itself cleanly redistributable and its inputs accounted for — the same property, applied to the tool, that the tool offers for corpora.

**Determinism (`CLAUDE.md` §7; see `deterministic-by-construction.md` and `cross-target-determinism.md`).** Attestrum's core claim is byte-identical output across machines and re-runs. Byte-determinism is a property of the *entire* build, dependency graph included — an uncontrolled dependency posture (a floating wildcard version, an unpinned git source resolving to a moving branch) is a direct threat to reproducibility, because the bytes that go in determine the bytes that come out. The `bans` wildcard-deny and the `sources` git-pin governance are determinism controls as much as they are security controls: they keep the dependency set pinned and named, which is a precondition for the lockfile-driven reproducibility the determinism matrix asserts.

The throughline: **a provenance tool's credibility is only as good as the provenance of the tool.** The dependency gates are where Attestrum applies its own standard to itself.

---

## 8. Limitations — what these gates do *not* establish

In the spirit of the project's other research docs, the claims here are scoped:

- **These gates are not a complete software bill of materials for the tool itself.** They govern license, source, advisory, and ban policy over the resolved graph; they do not emit a signed SBOM of the Attestrum binary, and nothing here asserts that the build is reproducible end-to-end on its own — that is the determinism work, cross-referenced above, not this.
- **`advisories` is only as complete as RUSTSEC.** A vulnerability with no published advisory is invisible to the check. A green `advisories` run means "no *known, published* advisory applies," not "no vulnerability exists."
- **The gates are build-time, not runtime.** They say nothing about the behavior of the binary in operation; they constrain what goes *into* the build, not what the build *does*.
- **An ignored advisory is a human judgment, not a proof.** §6's threat-model arguments are reasoned acceptances reviewed by a person; they are as good as that reasoning and the assumptions behind it (e.g., that the signing flow stays keyless-ephemeral). A change to the tool's actual usage could invalidate an acceptance, which is why each ignore is commented with the assumption it rests on.
- **The local/CI split trusts CI as the line of record.** The local hook is an optimization; a contributor without the hook installed relies entirely on CI. The split is sound only because CI runs the authoritative superset on the path to `main`.

None of these limits undercut the theses. The placement rule (§3) holds regardless of RUSTSEC's completeness, and the trust-inheritance argument (§7) is about applying the project's own standard to its construction — a standard these gates advance without claiming to complete.

---

## Companions

- `deterministic-by-construction.md` — the mechanisms behind byte-identical seals; the determinism that §7 ties the `bans`/`sources` discipline to.
- `cross-target-determinism.md` — which emitted fields are byte-identical across platforms, and the one that isn't.
- `provenance-without-disclosure.md` — the vendor-neutrality and "verify without the tool" promises §7 builds on.
- `specification-first-agentic-engineering.md` — the broader build methodology (diagram-first, plan-first, machine-checked gates) of which these dependency gates are one instance.
