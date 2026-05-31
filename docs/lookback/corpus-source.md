# Lookback corpus source — WikiText-103 (raw)

The Lookback demo (see `docs/diagrams/lookback/lookback-architecture.md`) seals the
**WikiText-103 raw** training corpus. This note records exactly which bytes are
sealed so that anyone can reproduce the build and verify they started from the same
input. **The corpus data itself is never committed to this repository** — only this
provenance record is.

## Source

| Field | Value |
|---|---|
| Hugging Face dataset | [`Salesforce/wikitext`](https://huggingface.co/datasets/Salesforce/wikitext) |
| Config | `wikitext-103-raw-v1` |
| Split | `train` |
| Format | Apache Parquet, single `text` column (`string`) |
| License | `CC-BY-SA-3.0` (Wikipedia text) |

The **raw** variant is deliberate: it keeps the original Wikipedia text (no
`<unk>` vocabulary substitution). It is still **moses-tokenized** (` @-@ `, spaced
punctuation), which is why the seal generator detokenizes each passage back to
natural English before sealing — see `crates/attestrum-pipeline/examples/wikitext/segment.rs`
and the rationale in `docs/diagrams/lookback/wikitext-seal-pipeline.md`. Detokenization
lives in segmentation only; it never touches the protected fingerprint normalization
(CLAUDE.md §4).

## Shards (train split — 2 files, ~314 MB total)

Resolved from the dataset's auto-converted Parquet branch
(`refs/convert/parquet`):

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `train/0000.parquet` | `156987808` | `74da360f23826045b3e6ac6375411fdb15f003030aa74f2596ed08b857cb9212` |
| `train/0001.parquet` | `157088770` | `ba090ac30dbf5461e8dcbdd1a1b8e6f3cf9c2c756d64f0c1220450acd514f720` |

Each SHA-256 equals the file's Hugging Face Git-LFS object id (the `lfs.oid`
reported by the HF tree API), so a re-fetch can be verified against Hugging Face
without trusting this repository.

URLs:

- `https://huggingface.co/datasets/Salesforce/wikitext/resolve/refs%2Fconvert%2Fparquet/wikitext-103-raw-v1/train/0000.parquet`
- `https://huggingface.co/datasets/Salesforce/wikitext/resolve/refs%2Fconvert%2Fparquet/wikitext-103-raw-v1/train/0001.parquet`

> Note: `refs/convert/parquet` is Hugging Face's auto-generated Parquet mirror of the
> dataset. The bytes are pinned by the SHA-256 above; if HF ever regenerates the
> mirror with different bytes, the hashes here are the source of truth for what was
> sealed.

## Local layout (gitignored — never committed)

The shards live under a repo-local working directory that the `.gitignore` tier-3
`/_*` glob (CLAUDE.md §0.5.5) already excludes:

```
_lookback-data/wikitext-103-raw-v1/train/0000.parquet
_lookback-data/wikitext-103-raw-v1/train/0001.parquet
```

## Re-fetch + verify

```bash
mkdir -p _lookback-data/wikitext-103-raw-v1/train
cd _lookback-data/wikitext-103-raw-v1/train
base="https://huggingface.co/datasets/Salesforce/wikitext/resolve/refs%2Fconvert%2Fparquet/wikitext-103-raw-v1/train"
curl -sSL -o 0000.parquet "$base/0000.parquet"
curl -sSL -o 0001.parquet "$base/0001.parquet"
shasum -a 256 0000.parquet 0001.parquet   # must match the table above
```

The sealed corpus is then produced by the seal generator:

```bash
cargo run -p attestrum-pipeline --release --example seal-wikitext \
  -- _lookback-data/wikitext-103-raw-v1/train _lookback-out
```

## Canonical seal (input → output, closeable)

Sealing the pinned input shards above with the seal generator yields this canonical
result. A verifier who re-runs the command on the byte-identical input must reproduce
the same Merkle root.

| Field | Value |
|---|---|
| Merkle root (BLAKE3, RFC 6962) | `de95bddc1c0d17123b8ab5960b3300f31a713b8804e32184ed03e606696726dc` |
| Leaves (passages) / unique CAS objects | 822,559 / 808,823 |
| `manifest.parquet` SHA-256 | `eafa3dd76ff9bcebfa6e115a1886fa761dd7c376256b053dd9e6ac9634e275a0` |
| Sealed by | `attestrum-pipeline` example `seal-wikitext` (release) |

The root is printed to stdout by the seal command; the manifest lands at
`<output-dir>/.attestrum/manifests/manifest.parquet`. See
`docs/research/deterministic-by-construction.md` §4.1 for the reproducibility analysis.
This root has been **reproduced byte-for-byte on Linux** (GitHub Actions `ubuntu-24.04`,
x86_64/glibc) via the `lookback-seal-crosscheck` workflow — re-downloading the pinned
shards, re-sealing, and asserting the identical root, `manifest.parquet` SHA-256, and leaf
count. See §4.1 for the cross-platform result.
