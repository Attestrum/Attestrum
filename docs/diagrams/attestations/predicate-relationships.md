---
title: "attestation predicate relationships"
models: "attestrum.com/attestation/training-corpus/v0.1, attestrum.com/attestation/inclusion-proof/v0.1, attestrum.com/attestation/non-inclusion-proof/v0.1, attestrum.com/attestation/takedown/v0.1; crates/attestrum-attest/src/predicates.rs"
source_of_truth: diagram
last_verified: bootstrap 2026-05-24
diagram_type: flowchart
---

# Predicate relationships

Source of truth flips to `code` when the predicate Rust types land in Sprint 4 (`training-corpus`) and Sprint 5 (`inclusion-proof`, `non-inclusion-proof`). `takedown/v0.1` lands in Sprint 6.

The three sister predicates form a DAG rooted at `training-corpus`. Each proof attestation embeds the corpus attestation digest as an immutable reference; an attacker would have to break BLAKE3 collision resistance to forge a proof against a different corpus than the one the publisher signed. We submit all three predicate types to the in-toto vetted catalog (PATH-A-BRIEF §9.2).

The predicate URIs themselves are a protected system per CLAUDE.md §4 — schema changes require a version bump (`v0.4` since current is v0.3), a migration document, and an in-toto vetted catalog re-submission.

```mermaid
flowchart LR
  TC[training-corpus/v0.1<br/>signed when attestrum build completes] -- subject digest --> M[(manifest.parquet<br/>+ merkle.root)]
  IP[inclusion-proof/v0.1<br/>signed when attestrum prove finds a hit] -- corpus.attestationDigest --> TC
  NIP[non-inclusion-proof/v0.1<br/>signed when attestrum prove finds none] -- corpus.attestationDigest --> TC
  TD[takedown/v0.1<br/>signed when attestrum takedown runs] -- previousRoot --> TC
  TD -- newRoot --> TC2[training-corpus/v0.1 v_n+1]
```
