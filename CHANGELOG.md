# Changelog

All notable user-facing changes to Attestrum.

Attestrum is pre-MVP (Sprint 4 of 6). No versioned releases yet. The first tagged release will be `v0.1.0` at the end of Sprint 6. Until then this file tracks landed milestones.

## [Unreleased] — pre-MVP

### Fixed — Tooling

- Diagram-linter freshness check no longer counts docs-only commits (CHANGELOG.md, SESSION-LOG.md) against the 30-commit rolling window. Three boundary-slippage incidents in the project history (each costing a docs-only fix-forward commit which itself loaded the window further) are now structurally impossible. Three new integration tests under `tools/diagram-linter/tests/freshness_pathspec.rs` pin the behavior.
- `docs/diagrams/sprint-4/sign-flow.md` — manual semantic re-read against the currently pinned fork rev fixed three drift items the structural diagram-linter cannot catch: the `source_of_truth` prose was updated to past tense to match the frontmatter flip that already landed; the Fulcio request was relocated to session construction (where it actually happens) rather than being attributed to `sign_dsse` internals; and the bundle's `verification_material.content` shape was updated from `X509CertificateChain[leaf]` to single-leaf `Certificate(leaf)` per the Bundle v0.3 spec requirement enforced by sigstore-go validators. Stale fork-rev references at two diagram lines were rephrased to point at the workspace `Cargo.toml` so the diagram doesn't restale on the next rev bump.

### Added — Sprint 4 (signing + verification)

- `attestrum sign` — emits a DSSE-wrapped Sigstore Bundle v0.3 over an in-toto Statement v1 payload, with the signing identity recorded in a Rekor v1 transparency-log entry (kind = `dsse`, version = `0.0.1`). The bundle round-trips through `cosign v3+ verify-blob-attestation --new-bundle-format` end-to-end without Attestrum installed (CI-verified).
- `attestrum verify` — local-only verifier for Attestrum bundles. Validates signature, certificate chain, Rekor inclusion, and Attestrum identity policy regex.
- Three predicate types defined at `https://attestrum.com/attestation/{training-corpus,inclusion-proof,non-inclusion-proof}/v0.3` with JSON-Schema derivation via `schemars`. `training-corpus` is fully populated by `attestrum sign`; the two proof predicates have URIs locked + types defined but payloads populated by `attestrum prove` in Sprint 5.
- `crates/attestrum-attest` — public Rust API for the predicate types, in-toto Statement wrapping, and bundle assembly. Stable surface guarded by a golden-file API-surface snapshot test.

### Added — Sprint 3 (manifest + pipeline + CLI)

- Parquet manifest schema (PROTECTED): 16 columns from the original spec plus two binding columns (`input_ordinal`, `occurrence_index`). Deterministic Parquet writer + reader pinned to zstd level 3.
- `attestrum build` — compiles a corpus from a `corpus.toml` spec into a sealed deterministic artifact.
- `attestrum inspect` — read-only inspector for Attestrum-shaped Parquet manifests.
- `attestrum plan` / `attestrum merge` — sub-corpus sharding for deterministic parallel builds.
- Three-stage Rayon fold-reduce pipeline that wires the signal parsers + hash/CAS/Merkle layer + manifest writer into a single deterministic build.

### Added — Sprint 2 (cryptographic foundation, PROTECTED)

- `crates/attestrum-cas` — content-addressed store with two-level hex sharding (`aa/bb/`). Atomic-rename single-put write contract. Tested to 50M objects.
- `crates/attestrum-merkle` — RFC 6962 binary Merkle over BLAKE3 with the multiset duplicate-leaf policy and audit-path index convention.
- BLAKE3 + SHA-256 dual-hash streaming hasher (8 KiB tee). BLAKE3 for CAS addressing and Merkle leaves; SHA-256 for Sigstore interop.
- 4-target byte-identical determinism CI matrix: linux-x86_64-glibc, linux-aarch64-glibc, macos-aarch64-darwin, linux-x86_64-musl.

### Added — Sprint 1 (workspace + top-3 signal parsers)

- Rust workspace at edition 2021, resolver=2, `rustc` 1.85.0.
- Three signal parsers with state-machine semantics: `robots.txt` (RFC 9309), `ai.txt` (Spawning), `tdmrep` (W3C, May 2024). Each emits a per-document `SignalVerdict` aggregated by the cross-parser pipeline.
- Custom diagram-linter (`tools/diagram-linter/`) enforcing six checks on every PR: Mermaid parse, frontmatter completeness, `last_verified` SHA freshness within 30-commit window, forward-reference resolution, reverse-reference coverage, and code-vs-diagram drift detection.
- `attestrum-core` — shared primitive types (`DocumentDigest`, `Modality`, `SourceType`, `SignalVerdict`, `Ruleset`).

### Added — Sprint 5 (fingerprinting, PROTECTED, in-progress)

- `crates/attestrum-fingerprint` — modality-routed perceptual fingerprinting. Text via MinHash-128 + SimHash-64 (hand-rolled over BLAKE3, no new external deps). Image via pHash + blockhash. ISCC composition via `iscc-lib` 0.4. Audio / video / PDF deferred to Sprint 5 mid/late.

### Added — Infrastructure

- Dual-license under Apache-2.0 OR MIT. Copyright © Hyper Beam Media LLC. Per-file SPDX headers omitted — the root `LICENSE-APACHE` + `LICENSE-MIT` files are authoritative.
- `cargo deny check sources licenses` as a pre-commit gate (sources whitelist for git pins, licenses whitelist matching `LICENSE-APACHE`).
- `cargo deny check advisories` (CI-only; carries forward two transitive RUSTSEC advisories pending upstream fixes — see CI workflow output for current state).
- `cosign-interop` GitHub Actions workflow asserting Attestrum-signed bundles verify end-to-end through `cosign v3+ verify-blob-attestation --new-bundle-format`.
- `determinism` GitHub Actions workflow asserting byte-identical builds across the four-target matrix.

### Protected systems (corpus-incompatible if changed)

The following subsystems are stable contracts; modifying any of them requires explicit founder approval and a major-version migration:

- `crates/attestrum-merkle/` — RFC 6962 Merkle over BLAKE3.
- `crates/attestrum-attest/` predicate URIs — `attestrum.com/attestation/{training-corpus,inclusion-proof,non-inclusion-proof}/v0.3`.
- `crates/attestrum-cas/` directory layout — anything under `.attestrum/{objects,cas,manifests}/`.
- `crates/attestrum-ledger/` tile layout — append-only.
- `crates/attestrum-fingerprint/` text normalization — NFC + lowercase + whitespace collapse.
- `tests/golden/article53/` — EU Article 53 template golden files.

### Documentation

- 27 architecture diagrams under `docs/diagrams/` rendered inline by GitHub. See the `DIAGRAMS-OVERVIEW.md` meta-map for the recommended reading order.
- `docs/license-inventory.md` — full transitive license inventory for downstream supply-chain audit.
- `docs/migration/v0.2-to-v0.3-attestrum-rebrand.md` — predicate URI migration notes for any early v0.2 install.

## Pre-MVP roadmap

- **Sprint 5 (in progress)** — fingerprinting for `attestrum prove` (inclusion / non-inclusion proofs), EU Article 53 + Annex XI emit, Croissant JSON-LD + CycloneDX ML-BOM sidecar formats.
- **Sprint 6** — Hugging Face Hub publish flow, takedown-witness ledger, static `verify.html` browser-only verifier, `v0.1.0` end-to-end demo on Common Pile v0.1.
