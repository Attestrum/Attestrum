# Lookback corpus source — fineweb-edu sample-10BT

The fourth reference bundle (after WikiText-103, dolly-15k, and PG-19,
`pg19-corpus-source.md`) seals the **sample-10BT** subset of
**HuggingFaceFW/fineweb-edu**: 9,672,101 educational web pages filtered from
FineWeb (CommonCrawl-derived), 28.5 GB of compressed parquet. This note records
exactly which bytes are sealed so that anyone can reproduce the build and
verify they started from the same input. **The corpus data itself is never
committed to this repository** — only this provenance record is.

This is the ladder's first **sharded** rung: the corpus plus its CAS does not
fit one free runner's disk, so the `fineweb10bt-seal-crosscheck` workflow seals
it as a 14-job matrix — one job per upstream parquet file — and an
`attestrum merge` job combines the 14 shard manifests into the single canonical
manifest + Merkle root. The merged root is byte-identical to what an unsharded
build of the same rows would produce (the sharding contract,
`crates/attestrum-cli/tests/sharding.rs`). The `fineweb10bt-publish` workflow
re-runs the matrix and signs only if the canonical values reproduce.

## Source

| Field | Value |
|---|---|
| Hugging Face dataset | [`HuggingFaceFW/fineweb-edu`](https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu) |
| HF revision (pinned) | `87f09149ef4734204d70ed1d046ddc9ca3f2b8f9` |
| Subset | `sample/10BT/` — 14 parquet files, 28,518,193,415 bytes total |
| Rows (= sealed leaves) | 9,672,101 |
| Columns | `text, id, dump, url, file_path, language, language_score, token_count, score, int_score` |
| License | `ODC-By-1.0` (dataset), subject to the CommonCrawl Terms of Use |

**One parquet row = one sealed leaf; the sealed bytes are the `text` column
bytes exactly** — no rendering, no normalization, no added newline (the PG-19
exact-bytes philosophy applied to a column). The metadata columns (`id`,
`dump`, `url`, scores, …) are **not** sealed; they remain available upstream.
The `source_uri` backref is the row's own `id` (a `urn:uuid`, globally unique
and shard-invariant); `source_dataset_id` is `fineweb-edu`; `language` is
carried from the row. See
`docs/diagrams/lookback/fineweb10bt-seal-pipeline.md`. The seal never touches
the protected fingerprint normalization (CLAUDE.md §4).

**Attribution.** The publish path renders a source / attribution /
modification section on the dataset card from
[`fineweb10bt-attribution.md`](./fineweb10bt-attribution.md) (passed verbatim
to `attestrum publish --attribution-file`).

## Pinned data files

The 14 parquet files under `sample/10BT/` at the revision above, with their
upstream LFS SHA-256 digests. Each matrix shard job downloads exactly one file
and asserts its digest **before sealing** — upstream byte drift fails the run
before anything is signed.

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `000_00000.parquet` | `2152819114` | `b1ba7b2ce4cb5ea6ef42dca40263eabb85f37700d01693a68e9b30a31d78e871` |
| `001_00000.parquet` | `2152222432` | `3fcf2dc69cd52503986276d3d2d26a8c356d0f2ea28a0de4fdbda8cf87755693` |
| `002_00000.parquet` | `2151796315` | `547ae182d132c9f06b6ce63149567208ea9f57630bfd9b1a2938e504f0c9ebd7` |
| `003_00000.parquet` | `2152437524` | `22184e6eb25759ddd97783751ffc73e1705dfa2542e630dae1f2a8bac8ee6ddb` |
| `004_00000.parquet` | `2152338550` | `33557ddd87a07a4ae6fcaf7a4789c7b484e5cc0c273ca12a65b74200e6d8748b` |
| `005_00000.parquet` | `2152189947` | `e08d79927ecb377786572cd854817c748ceb8880878b1a0eb91abf8c85d505ea` |
| `006_00000.parquet` | `2152689867` | `554fa2613c9261d6c9c396caab881de3160175794c6c3e1856bf28a0e9cb9b76` |
| `007_00000.parquet` | `2150686637` | `4b7d1f697d4afff7f5f65cb7aa4d83acc6db716da4d46ea23399d0c50608c6e3` |
| `008_00000.parquet` | `2151274846` | `a6d9dcc0c72ecd7c7f0173c2d06b3a4249a2ccaa94961469d1a522ca689bbf1a` |
| `009_00000.parquet` | `2151913277` | `f836cd3c70b95776699eb6c356b2dbf702816e25dcf39992f5c80a29029d23c3` |
| `010_00000.parquet` | `2152798864` | `e5a2eae25f057f0856a10bfae314c6ca8ea8bb08456d2131e9e89b2b8305e2f6` |
| `011_00000.parquet` | `2152323681` | `db71cf0425bb3d1813a09f12ff1acd6dabdfb91a2e4f141960254ee5a7f036e7` |
| `012_00000.parquet` | `2152069689` | `08b47a3e1c25161f796d2f8dbf99ccf60affdebdcea4910833d0d5783315551f` |
| `013_00000.parquet` | `540632672` | `b393f51fefab26cd6f4c8f65707c1924f6666c4961a0ebebe04bb57f7ec832de` |

## Local layout (gitignored — never committed)

```
_lookback-data/fineweb-edu-10bt/000_00000.parquet … 013_00000.parquet
```

Covered by the `.gitignore` tier-3 `/_*` glob (CLAUDE.md §0.5.5).

## Re-fetch + verify (one shard or all)

```bash
rev=87f09149ef4734204d70ed1d046ddc9ca3f2b8f9
base=https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu/resolve/$rev/sample/10BT
mkdir -p _lookback-data/fineweb-edu-10bt && cd _lookback-data/fineweb-edu-10bt
for i in $(seq -w 0 13); do
  curl -sSL -o "0${i}_00000.parquet" "$base/0${i}_00000.parquet"
done
shasum -a 256 *.parquet            # must match the table above
```

The sealed shard manifests are then produced by the seal generator (per shard
or over the whole directory — the merged root is identical either way):

```bash
cargo run -p attestrum-pipeline --release --example seal-fineweb-edu \
  -- _lookback-data/fineweb-edu-10bt _lookback-fineweb10bt-out
```

## Canonical seal (input → output, closeable)

Sealing the pinned input above through the 14-shard matrix + `attestrum merge`
yields this canonical result. A verifier who re-runs the generator on the
byte-identical input — sharded any way, or unsharded — must reproduce the same
Merkle root (multiset invariance; `crates/attestrum-cli/tests/sharding.rs`).
Captured by `fineweb10bt-seal-crosscheck` `mode=capture`
([run 27428316052](https://github.com/Attestrum/Attestrum/actions/runs/27428316052))
on Linux x86_64/glibc, the signing platform.

| Field | Value |
|---|---|
| Merkle root (BLAKE3, RFC 6962), merged | `4cdf2491b9fbb0dc875fc06a6c94872b9f40b1c343860d92b8e0247f7032053c` |
| Leaves (rows) | 9,672,101 |
| Merged `manifest.parquet` SHA-256 | `fa6c082ccd5f4e1b4ad95ac8966cf156221fbb3483f3ba2bf48dad5dc38defa5` |
| Sealed by | `attestrum-pipeline` example `seal-fineweb-edu` (release, CI, 14-shard matrix) + `attestrum merge` |

## Scale evidence (measured, capture run 27428316052)

The ladder's first sharded rung and the Stage-4 calibration point. Measured on
free standard GitHub Actions runners (ubuntu-24.04, 4 vCPU / 16 GB RAM), whole
run **19.5 minutes end-to-end** for the 28.5 GB corpus:

| Metric | Measured |
|---|---|
| Matrix | 14 shard jobs, fully parallel; each ~8–9 min including toolchain + build |
| Per-shard seal (e.g. `000`: 726,000 rows / 3.47 GB text) | wall 5:12, peak RSS ~4.6 GiB (`ContentSource::Bytes` holds the shard's decoded text) |
| Shard manifest artifact | ~55 MB each; only manifests cross the job boundary — the CAS never leaves the runner |
| Merge (9,672,101 rows from 14 manifests) | wall **27.6 s**, peak RSS **8.7 GiB** (9,129,556 kB) |
| Merged `manifest.parquet` | ~770 MB |

**The Stage-4 datum:** merge held ~8.7 GiB at 9.67M rows (~940 B/row, roughly
2× the pre-run estimate). Linear extrapolation to the ~100M-row 286 GB rung is
~90 GiB — far beyond a 16 GB runner, so Stage 4 requires a streaming merge (or
a large-memory machine); staged pairwise merging does not help, since the final
stage still holds every row. Deferred with its own approval per the plan.

## Published (signed + live)

Sealed by the 14-shard matrix, merged, gated on the canonical triple, signed
keyless under the **Attestrum GitHub Actions identity** (§A9 — never a
personal one), and pushed by the `fineweb10bt-publish` workflow
([run 27431593593](https://github.com/Attestrum/Attestrum/actions/runs/27431593593))
on 2026-06-12. The pre-sign gate asserted the canonical triple above before
Fulcio was contacted; the pushed `attestrum/manifest.parquet`'s LFS SHA-256
equals the canonical value.

| Field | Value |
|---|---|
| Hugging Face dataset | [`Attestrum/fineweb-edu-sample-10BT-sealed`](https://huggingface.co/datasets/Attestrum/fineweb-edu-sample-10BT-sealed) |
| Predicate type | `https://attestrum.com/attestation/training-corpus/v0.3` |
| Sigstore bundle | `attestrum/bundle.sigstore.json` (v0.3) |
| Rekor logIndex (global) | `1804671018` |
| Rekor `integratedTime` | `1781285906` (2026-06-12) |
| Signing identity | `…/.github/workflows/fineweb10bt-publish.yml@refs/heads/main` (issuer `token.actions.githubusercontent.com`) |

A third party with no Attestrum installed verifies the signed manifest with
stock cosign. **One scale note:** at 844 MB the merged manifest exceeds
cosign's default 128 MiB blob-read cap (`size of layer exceeded the limit`),
so the blob's digest is computed with `shasum`/`sha256sum` and handed to
cosign via `--digest`/`--digestAlg` — cosign still performs every signature,
identity, Rekor, and in-toto-subject check:

```bash
sha=$(shasum -a 256 manifest.parquet | awk '{print $1}')
cosign verify-blob-attestation \
  --new-bundle-format \
  --type 'https://attestrum.com/attestation/training-corpus/v0.3' \
  --bundle bundle.sigstore.json \
  --certificate-identity-regexp '^https://github\.com/Attestrum/Attestrum/\.github/workflows/fineweb10bt-publish\.yml@refs/.+$' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  --digest "$sha" --digestAlg sha256
```

Verified `Verified OK` against the live HF artifacts on 2026-06-12 (cosign
v3.0.6, independent machine).
