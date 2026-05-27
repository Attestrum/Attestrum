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
- S5-D1 E5 — fingerprint crate public API surface frozen at v0.1. `FingerprintBundle`, `TextFingerprint`, `ImageFingerprint`, `IsccComposition`, plus the re-exported `Modality` enum, now derive `schemars::JsonSchema`. The canonical schema is published at `attestrum.com/fingerprint/v0.1.schema.json` and pinned in-repo at `docs/schemas/fingerprint-v0.1.schema.json`. A hand-rolled API-surface golden test (`tests/api_surface.rs`) catches accidental `pub` additions / renames / signature shifts. A cross-target byte-determinism gate (`tests/determinism.rs` + committed PNG fixtures + bundle JSON goldens) runs as part of the `cargo test --workspace` invocation on every target of the existing `determinism.yml` 4-matrix and fails if any target produces a byte-differing fingerprint of the same input. Perceptual-hash threshold assertions tightened from the placeholder `>= 8` bound to calibrated `>= 20` (pHash) / `>= 30` (blockhash).
- S5-D2 E1 — `attestrum-prove` public API surface lands: `ProofTarget`, `PerceptualHashes`, `ManifestSource`, `ProveOpts`, `ProofArtifact`, `ProofKind`, `AttestrumProveError`, plus the `pub fn prove()` contract. **Contract only — no functional behavior yet:** the `prove()` body is `unimplemented!()` pending E2-E8 (E2 lands local-Parquet exact match; E4 wires DSSE-sign as the MVP gate; E7 adds Hugging Face + URL sources; E8 lands the CLI subcommand and freezes the surface via a hand-rolled `tests/api_surface.rs` golden). Downstream consumers can now write integration code against the stable surface. Per CLAUDE.md §14, deps are minimal at E1 (`attestrum-core`, `attestrum-attest`, `attestrum-fingerprint`, `serde`, `thiserror`); `parquet` + `arrow` + `attestrum-manifest` + `attestrum-merkle` land at E2; `hf-hub` + `url` at E7.
- S5-D2 E2 — `attestrum-prove` local-Parquet exact-match path lands. `prove()` no longer panics for the exact-hash dispatch arms (`ProofTarget::Blake3`, `Sha256`, `Bundle` against `ManifestSource::Local`): it reads the Parquet manifest via `attestrum-manifest`, finds the leaf whose BLAKE3 / SHA-256 matches the caller's target, recomputes the corpus's RFC 6962 BLAKE3 Merkle root via `attestrum-merkle::merkle_root`, builds an `InclusionProofPredicate` with `MatchEvidence::ExactBlake3` / `ExactSha256`, wraps it in an `InTotoStatement` at predicate type `attestrum.com/attestation/inclusion-proof/v0.3`, and returns a `ProofArtifact { kind: Inclusion, confidence: 1.0, bundle_path: None, ... }`. **Placeholders carried at E2** (filled in by later E-commits): `predicate.audit_path` is `vec![]` (E3 lands the real audit-path); `bundle_path` is forced to `None` regardless of `opts.sign` (E4 lands DSSE-sign); `corpus.attestation_digest` is zeros-hex (refined at E4 alongside signing); `proof_generated_at` / `proof_generator_identity` are `None` (E4 fills these from `opts.source_date_epoch` + the OIDC identity). Non-exact `ProofTarget` variants (ISCC, Perceptual, Document) and non-local `ManifestSource` variants (HF, URL) panic with clear "S5-D2 E5+" / "E7" messages pending those E-commits. Target-absent (would-be non-inclusion) cases panic with "S5-D2 E6" pending E6. Multiple-leaf-match-on-same-digest (manifest multiset policy) surfaces as `AttestrumProveError::Ambiguous(N)` rather than picking arbitrarily. New E2 deps: `attestrum-manifest`, `attestrum-merkle`, `serde_json` (all already in the workspace dep graph; no new external transitive crates). Thirteen integration tests at `crates/attestrum-prove/tests/exact_match.rs` cover the happy path, ambiguity error, predicate-type URI, JSON round-trip, Merkle-root cross-check against an independent `attestrum_merkle::merkle_root` call, and the five deferred-feature panics.
- S5-D2 E3 — `attestrum-prove` real audit-path lands. `predicate.audit_path` is no longer `vec![]`; `prove()` now constructs `attestrum_merkle::MerkleTree::new(...)` once over the manifest's `document_id` column and uses it for both `corpus.merkle_root = tree.root()` AND `predicate.audit_path = tree.audit_path(leaf_index)?` (hex-encoded `Vec<String>`). The predicate is now **cryptographically self-contained**: any external verifier can re-derive the corpus's Merkle root from just `predicate.{leaf_hash, leaf_index, tree_size, audit_path}` via `attestrum_merkle::verify_audit_path` — no manifest re-read required. The single-leaf-tree edge case is preserved (`audit_path == []` because the leaf hash IS the root, per RFC 6962). Remaining placeholders carried at E3 (filled in by later E-commits): `bundle_path = None` regardless of `opts.sign` (E4); `corpus.attestation_digest` zeros-hex (E4); `proof_generated_at` / `proof_generator_identity` `None` (E4). No new deps (`attestrum-merkle` already landed at E2). Three new integration tests at `crates/attestrum-prove/tests/exact_match.rs` cover the single-leaf edge case, the verify-via-recompute round-trip on a 7-leaf unbalanced tree (exercises the RFC 6962 odd-count carry-up rule), and a corrupted-path negative case confirming `verify_audit_path` actually exercises the proof.
- **S5-D2 E4 — `attestrum-prove` DSSE-sign (MVP gate).** When `opts.sign=true`, `prove()` canonicalizes the in-toto Statement via `InTotoStatement::canonical_json`, calls `attestrum_attest::sign` (Sigstore Bundle v0.3, DSSE envelope, Fulcio ephemeral cert, Rekor v1 dsse@0.0.1 transparency entry), writes the bundle to `<opts.workspace.or($PWD/.attestrum)>/prove/inclusion-proof.sigstore.json`, and returns `ProofArtifact { bundle_path: Some(_), ... }`. **First demonstrable signed inclusion proof verifiable via `cosign v3+ verify-blob-attestation --new-bundle-format` end-to-end without Attestrum installed.** When `opts.sign=false`, all of E3's behavior is preserved verbatim (`bundle_path: None`, no network, no OIDC) — the unsigned path stays determinism-test-friendly. Two additional predicate fields land at E4 (signed or unsigned): `proof_generated_at` populated from `opts.source_date_epoch` via `jiff::Timestamp::from_second(...).to_string()` (RFC 3339, deterministic); `corpus.attestation_digest` populated from the new `opts.corpus_bundle_path` field when present (BLAKE3 + SHA-256 via `attestrum_cas::stream_hash_path`), otherwise stays at the E2 zeros-hex placeholder. `proof_generator_identity` stays `None` even when signed — the bundle's leaf cert is the authoritative identity, populating the predicate field would require pre-sign JWT parsing (new dep, redundant with the cert) or a circular two-pass sign scheme. ProveOpts gains one new field: `corpus_bundle_path: Option<PathBuf>`. Caller-contract violations (`opts.sign=true` with `opts.oidc_id_token=None`) surface as `AttestrumProveError::Sign(AttestrumAttestError::SigstoreIdentityToken(...))` — no new error variants added, PATH-A-BRIEF §2.2's 6-variant lock stays intact. New E4 deps: `attestrum-cas`, `jiff` (both already in the workspace dep graph). Three new integration tests at `crates/attestrum-prove/tests/sign_integration.rs`: caller-contract violation, `attestation_digest` population from a corpus-bundle fixture (independent stream_hash_path cross-check), and a `#[ignore]`d signed-prove end-to-end test that runs only when `SIGSTORE_ID_TOKEN` is set (mirrors the `crates/attestrum-attest/tests/cosign_interop.rs` pattern). Adding a `cosign verify-blob-attestation` round-trip step to `.github/workflows/cosign-interop.yml` for the prove signed path is deferred to a follow-on commit.

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
