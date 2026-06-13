# Attestrum

A deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles.

Attestrum takes a directory of training data — text, images, audio, video, code — and emits a sealed, deterministic, Sigstore-signed bundle that says exactly what's in it, where it came from, and what consent rules applied. Anyone with `cosign v3+` (no Attestrum install required) can verify the bundle end-to-end against a transparency-log entry.

Attestrum is for anyone publishing a training corpus who wants it independently auditable without writing one-off README files — frontier labs included. It produces cryptographic, audit-ready evidence backing the EU AI Act Article 53(1)(d) training-content summary: provable corpus composition with inclusion and non-inclusion proofs. The bundle format is standards-based — Sigstore Bundle v0.3, in-toto Statement v1, RFC 6962 Merkle over BLAKE3 — so there is no Attestrum-only lock-in for downstream consumers.

## Guide

**New here? Start with the [Guide](./docs/guide/) — `build → sign → publish → verify`, end to end.** It covers the recommended path (seal and sign on Linux / CI, where keyless signing and fast `fsync` both live), the exact commands, a copy-paste GitHub Actions workflow, and verifying with stock `cosign`.

## Status

Pre-MVP. Sprint 4 of 6 just landed (DSSE-wrapped Sigstore Bundle v0.3 + Rekor v1 `dsse@0.0.1` transparency-log entry, cosign-verified end-to-end). Sprint 5 (fingerprinting + EU Article 53 emit) is in progress. The first tagged release will be `v0.1.0` at the end of Sprint 6.

No external API or storage stability promises until `v0.1.0`. The PROTECTED subsystems listed in `CHANGELOG.md` are corpus-incompatible if changed and will follow major-version migration discipline once shipped.

## Repository layout

```
crates/                 Rust workspace — 18 crates implementing the build pipeline,
                        Merkle tree, manifest writer, signing/verify, and the CLI.
tools/                  Workspace-internal tooling:
                          diagram-linter/   enforces docs/diagrams/ frontmatter + freshness
                          secret-scanner/   pre-commit credential pattern gate
                          fuzzy-web-gen/    generates the bounded near-match demo corpus artifact
tests/                  Cross-crate integration tests + golden fixtures.
docs/                   Public-facing docs:
                          diagrams/         47 Mermaid architecture diagrams
                          migration/        version migration notes
                          schemas/          JSON Schema files for predicate types
                          license-inventory.md
.github/workflows/      CI definitions (ci, cosign-interop, determinism).
.githooks/              Pre-commit hook wrapping the six-gate ritual (CLAUDE.md §7).
                        Activate per clone with `git config core.hooksPath .githooks`.
LICENSE-APACHE          Dual license — Apache-2.0 OR MIT
LICENSE-MIT             at your option. Copyright © Hyper Beam Media LLC.
README.md, CHANGELOG.md, SECURITY.md, CLAUDE.md, DIAGRAMS-OVERVIEW.md
```

Everything else in the working tree is local-only and gitignored — see [CLAUDE.md §0.5](./CLAUDE.md) for the three-tier publication-boundary cadence (external sibling `~/Documents/Claude/Attestrum-<purpose>/` for persistent local content; in-repo dotfile prefix `.<name>/` for tool caches; in-repo underscore prefix `_<name>/` reserved for future project-local working dirs).

## Build

```bash
cargo build --workspace
cargo test --workspace
```

Requires Rust 1.85+ (toolchain pinned via `rust-toolchain.toml`).

## CLI subcommands

```bash
attestrum build    --corpus <corpus.toml> --workspace <dir>          # compile a corpus into a deterministic sealed manifest
attestrum inspect  <manifest.parquet>                                # read-only manifest inspector
attestrum diff     <old.parquet> <new.parquet> [--out <report.json>] # read-only corpus-version delta (added/removed/unchanged + shift)
attestrum plan     --corpus <corpus.toml> --shards <n> --out <dir>   # deterministic sub-corpus sharding
attestrum merge    --inputs '<dir>/shard-*.parquet' --out <path>     # merge sharded builds (merged root == unsharded)
attestrum sign     <manifest> --source-date-epoch <ts>               # keyless Sigstore Bundle v0.3 (needs an OIDC id_token)
attestrum publish  --target huggingface|static --dataset <org/name>  # publish the verifiable artifact set
attestrum prove    <doc-or-blake3-hex> --against <manifest>          # inclusion / non-inclusion proof
attestrum verify   <bundle> --manifest <m> --certificate-identity <re>   # full verifier (cert + DSSE + Rekor + schema)
```

Plus `attestrum bind` / `attestrum walk-chain` for model-to-corpus binding and chain-walk verification. Run any subcommand with `--help` for the full flag set.

**For the end-to-end `build → sign → publish → verify` walkthrough — with exact flags and a copy-paste CI workflow — see the [Guide](./docs/guide/).**

## Architecture

Every module, CLI subcommand, public data structure, and multi-party flow has a Mermaid diagram under `docs/diagrams/`. GitHub renders them natively. Start with [`DIAGRAMS-OVERVIEW.md`](./DIAGRAMS-OVERVIEW.md) for the recommended reading order across the diagram set.

The workspace is 18 Rust crates under `crates/`:

- `attestrum-core` — shared primitives.
- `attestrum-signals` — robots.txt / ai.txt / TDMRep parsers.
- `attestrum-cas` — content-addressed store (PROTECTED layout).
- `attestrum-merkle` — RFC 6962 over BLAKE3 (PROTECTED).
- `attestrum-manifest` — Parquet manifest writer/reader (PROTECTED schema).
- `attestrum-pipeline` — Rayon fold-reduce build pipeline.
- `attestrum-attest` — in-toto Statement + Sigstore Bundle assembly.
- `attestrum-bind` — corpus-to-model binding (`model-binding/v0.1` in-toto Statement).
- `attestrum-fingerprint` — perceptual fingerprinting (PROTECTED text normalization).
- `attestrum-text-minhash` — PROTECTED text-MinHash kernel (`normalize_text` + 128-perm MinHash), extracted from `attestrum-fingerprint` for byte-identical `wasm32` reuse.
- `attestrum-fingerprint-wasm` — `cdylib` compiling the text-MinHash kernel to `wasm32` via a raw `extern "C"` ABI, so the attestrum.com near-match demo runs the identical Rust in the browser (byte-identity gated in CI).
- `attestrum-prove` — inclusion / non-inclusion proof builder.
- `attestrum-emit` — Croissant JSON-LD + CycloneDX ML-BOM + dataset-card + verify.html emitters.
- `attestrum-publish` — Hugging Face Hub + static-bundle publish targets.
- `attestrum-index` — derived fuzzy-lookup LSH sidecar indexes (discovery-grade acceleration for `prove`).
- `attestrum-cli` — user-facing CLI binary.
- Remaining crates (`attestrum-ledger`, `attestrum-fingerprint-registry`) ship in a later sprint.

## Determinism

Byte-identical builds across `linux-x86_64-glibc`, `linux-aarch64-glibc`, `macos-aarch64-darwin`, and `linux-x86_64-musl` are an invariant, not an aspiration. The `determinism.yml` GitHub Actions workflow asserts byte-identity on every push.

Sources of non-determinism are bugs: map iteration goes through `BTreeMap` or explicitly sorted `Vec`; timestamps come from a single `--source-date-epoch` parameter; no floating-point arithmetic in any hash or Merkle path; Parquet pinned to `zstd` level 3.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))
- MIT License ([LICENSE-MIT](./LICENSE-MIT))

at your option.

Copyright © Hyper Beam Media LLC.

## Security

See [SECURITY.md](./SECURITY.md) for the vulnerability disclosure policy.

Determinism bugs, Merkle-root bugs, and signature-correctness bugs are security-class: a wrong byte in any of these surfaces invalidates every prior bundle. Report via the disclosure channel.

## Contributing

See [CLAUDE.md](./CLAUDE.md) for the working-discipline rulebook: plan-first development, diagrams-before-code, the five pre-commit gates, the protected-systems policy, and the per-commit push cadence. The same rules apply to outside contributors.
