---
title: "diagram-linter CI sequence"
models: "tools/diagram-linter/src/main.rs, tools/diagram-linter/src/lib.rs, .github/workflows/ci.yml"
source_of_truth: code
last_verified: ea9489b 2026-05-30
diagram_type: sequenceDiagram
---

# diagram-linter — CI sequence

Source of truth: `code` — linter is feature-complete as of Sprint 1 E6 and the CI workflow lands at Sprint 1 E11. All six PATH-A-BRIEF §0.3 checks implemented (parse + frontmatter + freshness + reverse-refs + forward-refs + drift); CI invokes `cargo run -p diagram-linter --release --quiet -- check --strict --root docs/diagrams` after pinning Node 20 + `@mermaid-js/mermaid-cli@10.9.1` and Rust 1.89.0 (bumped from 1.84.0 → 1.85.0 at Sprint 2 E2 → 1.88.0 at Sprint 4 E3 → 1.89.0 at the chore(toolchain) precursor to S5-D3 E2).

**Determinism CI** (4-target matrix per BUILD-PLAN §6.5) is a separate workflow at `.github/workflows/determinism.yml` (Sprint 2 E3, extended at Sprint 2 E9 with Merkle-root assertion and Sprint 3 E8 with manifest.parquet byte-identity). This diagram covers `ci.yml` only; determinism CI's flow is described in the corresponding sprint diagrams + workflow comments.

**Sprint 1 dep policy:** linter ships dependency-free using only `std` + workspace-approved `serde_json` + `std::process::Command` for `mmdc`, `cargo metadata`, and `git`. Recursive directory walking uses `std::fs::read_dir`. Frontmatter is `---` delimited and parsed line-by-line (no full YAML parser needed for the four flat string fields).

**Bootstrap exception:** the freshness check (Check 3) accepts `last_verified: bootstrap YYYY-MM-DD` as valid until the first non-bootstrap commit has been merged. After that, only `<short-sha> YYYY-MM-DD` matching the recent commit window is accepted.

```mermaid
sequenceDiagram
  autonumber
  participant CI as GitHub Actions
  participant L as diagram-linter binary
  participant FS as docs/diagrams/**/*.md
  participant MM as mmdc<br/>(npm pin 10.9.1, SHA-verified)
  participant CM as cargo metadata
  participant GIT as git log + git rev-parse

  CI->>L: cargo run -p diagram-linter -- check --strict --json
  L->>FS: walk via std::fs::read_dir
  FS-->>L: file list

  loop for each diagram file
    L->>L: split on /^---$/ → frontmatter + body
    L->>L: parse 4 required keys<br/>title, models, source_of_truth, last_verified
    Note over L: CHECK 2 — frontmatter present + complete

    L->>L: extract ```mermaid``` fenced block
    L->>MM: pipe body, expect exit 0
    MM-->>L: parse OK or stderr
    Note over L: CHECK 1 — mermaid parses cleanly

    L->>L: parse last_verified value
    alt value == "bootstrap YYYY-MM-DD"
      L->>L: accept iff project has no non-bootstrap commits yet
    else value == "<sha> YYYY-MM-DD"
      L->>GIT: rev-list HEAD~30..HEAD<br/>+ merge-base HEAD origin/main
      GIT-->>L: recent SHAs + PR range
      L->>L: assert sha ∈ window
    end
    Note over L: CHECK 3 — freshness
  end

  L->>CM: cargo metadata --no-deps --format-version=1
  CM-->>L: workspace packages + manifest paths
  L->>FS: scan crates/**/src/lib.rs + mod.rs
  FS-->>L: grep `^pub (mod|struct|trait|fn|enum|type|const|static) (\w+)`
  L->>L: for each pub item, check ≥1 diagram references it<br/>(in `models:` or as a node label)
  Note over L: CHECK 5 — reverse refs<br/>(generated + #[doc(hidden)] exempt)

  L->>FS: re-scan diagrams for tokens matching<br/>PascalCase::snake_case or crate::path::Item
  FS-->>L: token list per diagram
  L->>CM: confirm each token exists in workspace
  CM-->>L: hit / miss
  Note over L: CHECK 4 — forward refs<br/>(dangling = fail)

  L->>GIT: log --pretty=format:%h -- crates/**<br/>scoped to current PR
  GIT-->>L: changed code files
  L->>L: for each changed file w/ source_of_truth: code,<br/>verify referencing diagrams touched in same PR
  Note over L: CHECK 6 (PATH §0.3 #5) — drift

  alt all checks pass
    L-->>CI: exit 0
  else any check fails
    L-->>CI: exit 1 + JSON report uploaded as CI artifact
  end
```

**Layered build order (commits E3 → E6):**

- E3: binary scaffold, clap `check --strict --json` recognized, all checks return `Ok` no-op.
- E4: Checks 1 + 2 (mermaid parse + frontmatter). Tests via golden fixtures under `tools/diagram-linter/tests/fixtures/{ok,bad-frontmatter,bad-mermaid}/`.
- E5: Checks 3 + 5 (freshness + reverse refs). Bootstrap token logic. Tests cover both bootstrap and SHA cases.
- E6: Checks 4 + 6 (forward refs + drift). Linter is now feature-complete per PATH-A-BRIEF §0.3.
