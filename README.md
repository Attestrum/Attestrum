# Attestrum

A deterministic Rust CLI that compiles AI training corpora into cryptographically verifiable provenance bundles.

Attestrum takes a directory of training data — text, images, audio, video, code — and emits a sealed, deterministic, Sigstore-signed bundle that says exactly what's in it, where it came from, and what consent rules applied. Anyone with `cosign v3+` (no Attestrum install required) can verify the bundle end-to-end against a transparency-log entry.

Attestrum is for anyone publishing a training corpus who wants it independently auditable without writing one-off README files — frontier labs included. It produces cryptographic, audit-ready evidence backing the EU AI Act Article 53(1)(d) training-content summary: provable corpus composition with inclusion and non-inclusion proofs. The bundle format is standards-based — Sigstore Bundle v0.3, in-toto Statement v1, RFC 6962 Merkle over BLAKE3 — so there is no Attestrum-only lock-in for downstream consumers.

## Status

Pre-MVP. Sprint 4 of 6 just landed (DSSE-wrapped Sigstore Bundle v0.3 + Rekor v1 `dsse@0.0.1` transparency-log entry, cosign-verified end-to-end). Sprint 5 (fingerprinting + EU Article 53 emit) is in progress. The first tagged release will be `v0.1.0` at the end of Sprint 6.

No external API or storage stability promises until `v0.1.0`. The PROTECTED subsystems listed in `CHANGELOG.md` are corpus-incompatible if changed and will follow major-version migration discipline once shipped.

## Repository layout

```
crates/                 Rust workspace — 14 crates implementing the build pipeline,
                        Merkle tree, manifest writer, signing/verify, and the CLI.
tools/                  Workspace-internal tooling:
                          diagram-linter/   enforces docs/diagrams/ frontmatter + freshness
                          secret-scanner/   pre-commit credential pattern gate
tests/                  Cross-crate integration tests + golden fixtures.
docs/                   Public-facing docs:
                          diagrams/         28 Mermaid architecture diagrams
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

## CLI subcommands shipped so far

```bash
attestrum build    --corpus <corpus.toml> --workspace <dir>   # compile a corpus into a deterministic artifact
attestrum inspect  <manifest.parquet>                          # read-only manifest inspector
attestrum plan     --corpus <corpus.toml> --shards <n>        # deterministic sub-corpus sharding
attestrum merge    --shards <dir>                             # merge sharded sub-corpus builds
attestrum sign     <manifest> --predicate training-corpus     # emit DSSE-wrapped Sigstore Bundle v0.3
attestrum verify   <bundle>                                    # local-only verifier
```

`attestrum prove` (inclusion / non-inclusion proofs over the corpus) and `attestrum publish` (Hugging Face Hub publish flow) ship in Sprint 5 + 6.

## Architecture

Every module, CLI subcommand, public data structure, and multi-party flow has a Mermaid diagram under `docs/diagrams/`. GitHub renders them natively. Start with [`DIAGRAMS-OVERVIEW.md`](./DIAGRAMS-OVERVIEW.md) for the recommended reading order across the 27 diagrams.

The workspace is 14 Rust crates under `crates/`:

- `attestrum-core` — shared primitives.
- `attestrum-signals` — robots.txt / ai.txt / TDMRep parsers.
- `attestrum-cas` — content-addressed store (PROTECTED layout).
- `attestrum-merkle` — RFC 6962 over BLAKE3 (PROTECTED).
- `attestrum-manifest` — Parquet manifest writer/reader (PROTECTED schema).
- `attestrum-pipeline` — Rayon fold-reduce build pipeline.
- `attestrum-attest` — in-toto Statement + Sigstore Bundle assembly.
- `attestrum-fingerprint` — perceptual fingerprinting (PROTECTED text normalization).
- `attestrum-cli` — user-facing CLI binary.
- Remaining crates (`attestrum-ledger`, `attestrum-emit`, `attestrum-prove`, `attestrum-publish`, `attestrum-fingerprint-registry`) ship in Sprints 5–6.

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
