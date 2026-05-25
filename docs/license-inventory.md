# License inventory

Per CLAUDE.md §8: every external crate Attestrum depends on is tracked here with its SPDX license and the date the dependency was added. Append a row whenever a dependency lands. Sprint 1's policy (per founder-approved cross-check) is **zero new external dependencies** — `tools/diagram-linter` ships dep-free; `attestrum-core` lands with only pre-approved workspace deps.

## Pre-approved workspace dependencies

These are pre-approved by `BUILD-PLAN.md` §6.2 and `PATH-A-BRIEF.md` Part 2. Adding any of them to a crate is allowed without further approval; only ADDITIONS to this list need founder sign-off.

| Crate | Version | License (SPDX)        | Pre-approval source        |
|---|---|---|---|
| `blake3`       | `1.5`    | `Apache-2.0 OR CC0-1.0`         | BUILD-PLAN §6.2           |
| `sha2`         | `0.10`   | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `serde`        | `1`      | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `serde_json`   | `1`      | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `thiserror`    | `1`      | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `jiff`         | `0.x`    | `MIT OR Apache-2.0`             | PATH-A-BRIEF §2.1         |
| `unicode-normalization` | `0.1` | `MIT OR Apache-2.0`        | PATH-A-BRIEF §2.1         |
| `robotstxt`    | latest   | `Apache-2.0`                    | BUILD-PLAN §6.2           |
| `quick-xml`    | `0.31`   | `MIT`                           | BUILD-PLAN §6.2           |
| `clap`         | `4.5`    | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `tracing`      | latest   | `MIT`                           | BUILD-PLAN §6.2           |
| `tracing-subscriber` | latest | `MIT`                       | BUILD-PLAN §6.2           |
| `arrow`        | `52.x`   | `Apache-2.0`                    | BUILD-PLAN §6.2           |
| `parquet`      | `52.x`   | `Apache-2.0`                    | BUILD-PLAN §6.2           |
| `rocksdb`      | `0.22`   | `Apache-2.0 OR MIT OR BSD-3-Clause` | BUILD-PLAN §6.2       |
| `sigstore`     | latest   | `Apache-2.0`                    | BUILD-PLAN §6.2           |
| `tokio`        | `1.40+`  | `MIT`                           | BUILD-PLAN §6.2           |
| `reqwest`      | `0.12`   | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2           |
| `iscc-lib`     | `0.4`    | `Apache-2.0`                    | PATH-A-BRIEF §2.1         |
| `image`        | `0.25`   | `MIT OR Apache-2.0`             | PATH-A-BRIEF §2.1         |
| `image_hasher` | `3.0`    | `MIT`                           | PATH-A-BRIEF §2.1         |
| `blockhash`    | `0.5`    | `Apache-2.0 OR MIT`             | PATH-A-BRIEF §2.1         |
| `hf-hub`       | `git pin` | `Apache-2.0`                   | PATH-A-BRIEF §2.3 (only approved git-pinned dep) |
| `proptest`     | `1`      | `MIT OR Apache-2.0`             | Sprint 2 E2 plan (founder-approved 2026-05-23; dev-dep only; CLAUDE.md §7.1 obligation) |
| `rayon`        | `1`      | `MIT OR Apache-2.0`             | Sprint 3 E4 (founder-approved 2026-05-24; design-implied by BUILD-PLAN §4 + §9 + `rayon-pipeline.md` but absent from §6.2 crate table) |
| `toml`         | `0.8`    | `MIT OR Apache-2.0`             | BUILD-PLAN §6.2 (row was missing from this pre-approved table prior to Sprint 3 E5 — docs-sync gap, not a fresh approval) |

## Actually-used in this workspace

Append a row when a crate is added to ANY workspace member's `Cargo.toml`. Format: `<crate>@<version> | <SPDX> | <YYYY-MM-DD> | <added in commit> | <crate(s) that depend on it>`.

| Crate | Version | License (SPDX) | Date added | Commit | Used by |
|---|---|---|---|---|---|
| `serde`      | `1.x`  | `MIT OR Apache-2.0` | 2026-05-23 | E7 | `attestrum-core` |
| `thiserror`  | `1.x`  | `MIT OR Apache-2.0` | 2026-05-23 | E7 | `attestrum-core` |
| `serde_json` | `1.x`  | `MIT OR Apache-2.0` | 2026-05-23 | E7 | `attestrum-core` (dev-dep) |
| `proptest`   | `1.x`  | `MIT OR Apache-2.0` | 2026-05-23 | Sprint 2 E2 | `attestrum-signals` (dev-dep) |
| `blake3`     | `1.5`  | `Apache-2.0 OR CC0-1.0` | 2026-05-23 | Sprint 2 E5 | `attestrum-cas` |
| `sha2`       | `0.10` | `MIT OR Apache-2.0` | 2026-05-23 | Sprint 2 E5 | `attestrum-cas` |
| `arrow`      | `55.2` | `Apache-2.0` | 2026-05-24 | Sprint 3 E3 | `attestrum-manifest` (regular + dev) — bumped from BUILD-PLAN §6.2's `52.x` because `arrow 52.2.0 + chrono 0.4.44` had a `quarter()` trait-disambiguation compile error fixed in `arrow 53+` |
| `parquet`    | `55.2` | `Apache-2.0` | 2026-05-24 | Sprint 3 E3 | `attestrum-manifest` (regular + dev) — features `arrow + zstd`, `default-features = false`. Bumped from `52.x` per same reason as `arrow`. |
| `rayon`      | `1.x`  | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 3 E4 | `attestrum-pipeline` — `par_iter().fold(...).reduce(...)` per-worker accumulators for the hash + CAS-put stage per E3 cross-check. Founder-approved 2026-05-24 as a documented amendment to BUILD-PLAN §6.2 (was design-implied but missing from the crate table). |
| `clap`       | `4.x`  | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 3 E5 | `attestrum-cli` — derive-macro CLI framework for `attestrum build` (and later subcommands). `features = ["derive"]`. Pre-approved per BUILD-PLAN §6.2 and the inventory's pre-approved table above. |
| `tracing`    | `0.1`  | `MIT`               | 2026-05-24 | Sprint 3 E5 | `attestrum-cli` — structured logging facade. Pre-approved per BUILD-PLAN §6.2 and the inventory's pre-approved table. |
| `tracing-subscriber` | `0.3` | `MIT`        | 2026-05-24 | Sprint 3 E5 | `attestrum-cli` — `tracing` sink with `env-filter` + `fmt` features for the RUST_LOG-style verbosity flag. Pre-approved per BUILD-PLAN §6.2. |
| `toml`       | `0.8`  | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 3 E5 | `attestrum-cli` — corpus.toml deserialization per BUILD-PLAN §8.3. Pre-approved per BUILD-PLAN §6.2 (row was missing from this file's pre-approved table; added in this commit alongside the in-use row — docs-sync gap, not a fresh approval). |
| `schemars`   | `1.2`  | `MIT`               | 2026-05-24 | Sprint 4 E2.5 | `attestrum-attest` — derives JSON Schema 2020-12 from the three predicate Rust types via `#[derive(JsonSchema)]` + `schema_for!()`. Emits to `docs/schemas/{training-corpus,inclusion-proof,non-inclusion-proof}-v0.1.schema.json`. Founder-approved 2026-05-24 as a new dev/regular dep: not in BUILD-PLAN §6.2 explicit list but the standard Rust JSON-Schema derivation crate, MIT-licensed (allow-listed per CLAUDE.md §8). v1.x emits draft 2020-12 by default which matches PATH-A-BRIEF §3 schemas' `$schema` field; v0.8 emits draft 7 (wouldn't match). Added to `[workspace.dependencies]` in workspace Cargo.toml + `[dependencies]` in `crates/attestrum-attest/Cargo.toml`. |
| `sigstore`   | `0.14` | `Apache-2.0`        | 2026-05-24 | Sprint 4 E3 | `attestrum-attest` — Sigstore Bundle v0.3 sign + verify per BUILD-PLAN §3.4 / §6.2 + PATH-A-BRIEF Part 2. Pre-approved per BUILD-PLAN §6.2; pinned to v0.14 (current at 2026-05-24, has `bundle = [sign, verify]` feature flag for end-to-end Bundle v0.3 support). `default-features = false` + explicit features `bundle, fulcio, rekor, sigstore-trust-root, rustls-tls` to drop `cosign` (OCI), `mock-client`, `cached-client`, `oauth` (transitively brought in by `fulcio`), and to prefer `rustls-tls` over `native-tls` for cross-platform determinism. Requires rustc ≥ 1.88 transitively via `serde_with 3.20`; triggered the chore(toolchain) bump from 1.85 → 1.88 immediately before this commit. **Transitive-only license-allow-list expansions** added to `deny.toml` at the same commit per CLAUDE.md §8 transitive-exception pattern: `ISC` + `MIT-0` (from `aws-lc-rs` + `aws-lc-sys` via rustls), `Zlib` (from `foldhash` via newer hashbrown), `CDLA-Permissive-2.0` (from `webpki-root-certs`). All four are OSI-approved permissive; direct deps remain restricted to the CLAUDE.md §8 list. |
| `zstd`       | `=0.13.3` | `MIT`           | 2026-05-24 | Sprint 4 E3.6 | `attestrum-manifest` — declared-but-not-imported direct dep that exists purely to give the cargo resolver an exact-version constraint on the compression codec underlying `parquet`'s `zstd` feature. `parquet` is what actually compresses bytes (CLAUDE.md §7 PROTECTED: ZSTD level 3); this dep entry stops a future `cargo update` from silently bumping `zstd 0.13.3` → `zstd 0.13.x` (and transitively the bundled C `zstd-sys 2.0.12+zstd.1.5.6` → newer zstd-sys → different compressed bytes), which would break the 4-target byte-identity gate on `manifest.parquet`. Founder-approved 2026-05-24 as determinism-hardening dep; MIT is already on the allow-list so no deny.toml change. The transitive deps `zstd-safe 7.2.1` (MIT/Apache-2.0) and `zstd-sys 2.0.12+zstd.1.5.6` (MIT/Apache-2.0; bundles C zstd 1.5.6) inherit allow-list coverage. |
| `attestrum-attest` | `0.0.1` | path-dep `Apache-2.0 OR MIT` (workspace) | 2026-05-24 | Sprint 4 E3.5 | `attestrum-cli` — promoted internal path-dep with explicit `version = "0.0.1"` pin (per `feedback_attestrum_path_deps_need_version.md`). Provides `TrainingCorpusPredicate` + `InTotoStatement` + `deterministic_json` + `sign()` for the `attestrum sign` subcommand. |
| `sha2`       | `0.10` | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 4 E3.5 | `attestrum-cli` (promoted from `attestrum-cas`-transitive to direct) — feeds the SHA-256 half of the `DigestMap` for the in-toto Subject + the predicate's `manifest.digest_set` (BLAKE3 is Attestrum-native, SHA-256 is the in-toto / Sigstore interop requirement per BUILD-PLAN §3.4). Already in workspace deps. |
| `x509-cert`  | `0.2.5` | `Apache-2.0 OR MIT` | 2026-05-24 | Sprint 4 E4 | `attestrum-attest` — parses Sigstore Bundle's leaf cert (DER from `verificationMaterial.certificate.rawBytes` keyless OR `.x509CertificateChain.certificates[0].rawBytes` legacy chain form) to extract SAN + Fulcio OIDC-issuer extension (OID `1.3.6.1.4.1.57264.1.8` v1, fallback `1.3.6.1.4.1.57264.1.1` legacy). Promoted from sigstore-rs-transitive to direct so `attestrum_attest::identity::extract_identity` can operate on bundles without going through sigstore-rs's `pub(crate)` `CheckedBundle` internals. License already allow-listed; no `deny.toml` change. |
| `regex`      | `1.x`  | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 4 E4 | `attestrum-attest` — applies the operator-supplied `--certificate-identity <REGEX>` + `--certificate-oidc-issuer <REGEX>` patterns to the extracted identity for cosign-compatible verify semantics (sigstore-rs's `bundle::verify::policy::Identity` is literal-string-only). Promoted from `tracing-subscriber`-`env-filter`-transitive to direct. License pre-approved per BUILD-PLAN §6.2's general allow-list. |
| `base64`     | `0.22` | `MIT OR Apache-2.0` | 2026-05-24 | Sprint 4 E4 | `attestrum-attest` — decodes the base64-DER of `bundle.verificationMaterial.certificate.rawBytes` for the x509-cert parser. Promoted from multiple-transitive to direct. License pre-approved per CLAUDE.md §8. |

## GitHub Actions consumed

Tracked for supply-chain transparency. Actions are not linked into any Rust binary, so SPDX rows in the table above don't apply — but the consumed binary is run in CI on every triggered workflow execution, so the provenance is worth recording.

| Action | Pin | Date added | Workflow | Purpose |
|---|---|---|---|---|
| `sigstore/cosign-installer` | `@v3` (the ACTION's major-version pin — distinct from the cosign binary version it installs; explicit `cosign version --json` grep-and-exit-1 on binary-version drift inside the workflow) | 2026-05-24 | `.github/workflows/cosign-interop.yml` (Sprint 4 E4.5) | Installs the latest cosign binary (v2.5.2 at 2026-05-25; cosign has not shipped v3.x yet despite the parked-plan brief's assumption that it had) so the `cosign verify-blob-attestation --new-bundle-format` interop test can run against a real cosign binary on the runner. The `--new-bundle-format` flag landed in cosign v2.3.0; the workflow's binary-version gate accepts `v2.5+` or any future `v3+`. Belt-and-suspenders pin per parked-plan tactical decision D — `@v3` is the action major-version pin; the in-workflow `case` statement catches both installer regressions and any cosign binary-version drift below v2.5 or outside the v2/v3 lines. |
