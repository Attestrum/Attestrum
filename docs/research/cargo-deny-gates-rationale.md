# Cargo-deny gates — rationale

Why each `cargo deny` check is (or isn't) in the local pre-commit set. Migrated out of `CLAUDE.md` §7 on 2026-05-27 during a character-count audit. The rule itself — `cargo deny check sources licenses` as gate 5 of the six-gate pre-commit ritual — stays in CLAUDE.md §7; this file is the historical rationale.

## Why `cargo deny check sources licenses` is in the local pre-commit set

Added 2026-05-25 after Sprint 5 S5-D1 E1 through E4 + the `deny.toml` fix-forward + the parallel `difficulty.md` self-audit's §4.2.7 finding all surfaced the same gap.

The `sources` check catches `[patch.crates-io]` / git-pin additions whose URL isn't in `deny.toml`'s `allow-git` list. Regression seen at `60a78559` → `25e9d7e` fix-forward.

The `licenses` check catches transitive deps whose SPDX license isn't in `deny.toml`'s `allow` list. Regressions seen when first-using:

- `image` → `ravif` → `rav1e` → `libfuzzer-sys` (NCSA, at S5-D1 E2).
- `iscc-lib` → `xxhash-rust` (BSL-1.0, at S5-D1 E4).

Both checks run sub-second locally; both are policed by CI's `audit` job. Local pre-check stops the regression at commit time rather than after a wasted push.

## Why other `cargo deny` checks are NOT in the local pre-commit set

`cargo deny check bans` is omitted because it's a "ban-list" gate that the workspace doesn't currently populate — re-add once any `[bans].deny` entries land.

`cargo deny check advisories` is deliberately CI-only because it's slow (queries the RUSTSEC index) and currently red on two carry-forward transitive advisories:

- `RUSTSEC-2024-0436` — `paste` unmaintained.
- `RUSTSEC-2023-0071` — Marvin Attack (RSA timing side-channel).

Both are transitive deps pending upstream fixes; the carry-forward triage state is tracked alongside the CI workflow output.
