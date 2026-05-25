---
title: "signal parser pipeline (Sprint 1 — top three signals)"
models: "crates/attestrum-signals/src/lib.rs::SignalParser, crates/attestrum-signals/src/robots.rs, crates/attestrum-signals/src/ai_txt.rs, crates/attestrum-signals/src/tdmrep.rs, crates/attestrum-signals/src/decision.rs"
source_of_truth: diagram
last_verified: bootstrap 2026-05-24
diagram_type: flowchart
---

# Signal parser pipeline — Sprint 1

Source of truth flips to `code` in commit E10 when all three parsers + `decision.rs` aggregator land. Sprint 1 ships only the three highest-value parsers: robots.txt (RFC 9309), ai.txt (Spawning), and TDMRep (W3C, May 2024). AIPref, IPTC-PLUS, C2PA, RSL, Liccium, Cloudflare Content Signals are deferred to later sprints per BUILD-PLAN §9 Sprint 1 risk note.

**Sprint 1 fetch policy:** the parsers themselves take `&[u8]` + a `SignalContext`. They DO NOT fetch. Network is out of scope for Sprint 1 — `attestrum-fetch` was dropped from the workspace per PATH-A-BRIEF §1.10, and fetch orchestration is deferred to `attestrum-pipeline` in Sprint 3. Fixtures provide the bytes.

```mermaid
flowchart TD
  IN["source URL or local path<br/>(test fixture in Sprint 1)"]
  IN --> LOAD["load bytes<br/>(local fixture; no network)"]

  LOAD --> CTX["SignalContext<br/>{ host, path, http_status, http_headers }"]
  CTX --> DIS{signal kind?}

  DIS -->|robots.txt content| RP["robots::RobotsParser<br/>(uses google/robotstxt port)"]
  DIS -->|ai.txt content| AP["ai_txt::AiTxtParser<br/>(in-tree, ~150 LOC)"]
  DIS -->|TDMRep well-known JSON| TJ["tdmrep::WellKnownParser"]
  DIS -->|TDMRep HTTP header| TH["tdmrep::HeaderParser"]
  DIS -->|TDMRep HTML meta tag| TM["tdmrep::MetaTagParser"]

  RP --> SD["SignalDecision<br/>{ verdict, source_signal, reason }"]
  AP --> SD
  TJ --> SD
  TH --> SD
  TM --> SD

  SD --> AGG["decision::aggregate(decisions, ruleset)"]
  AGG --> OUT{aggregated verdict}
  OUT -->|all signals allow OR none disallow| INCL[Included]
  OUT -->|any signal denies| EXC[StrictReject<br/>or AuditFlag<br/>or PermissiveInclude]
  OUT -->|no signal expresses preference| UNK[Unknown<br/>routed by ruleset]
```

**Per-parser edge cases land in same-commit sub-diagrams:**

- `sprint-1/robots-txt-state.md` (commit E8): HTTP error → unknown; 404 → permissive; 200 empty body → permissive; explicit `Disallow: /` → disallowed.
- `sprint-1/ai-txt-rules.md` (commit E9): modality filters (`image/*`, `text/*`); operator-level vs path-level directives.
- `sprint-1/tdmrep-resolution.md` (commit E10): well-known JSON → HTTP header override → HTML meta override; protocol-error values treated as unset per W3C spec.

These three sub-diagrams are NOT required to exist before Sprint 1 code lands — they ship in the same commit as their parser code per CLAUDE.md §2 "update the diagram in the SAME commit as the code change."
