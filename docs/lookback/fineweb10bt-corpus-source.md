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

**Pending.** The canonical merged triple (Merkle root, merged
`manifest.parquet` SHA-256, leaf count) will be captured by the first
`fineweb10bt-seal-crosscheck` `mode=capture` run on Linux x86_64/glibc, the
signing platform, then pinned here and in the workflow `CANONICAL_*` env, and
reproduced by a full `mode=assert` matrix re-run before anything is signed.
