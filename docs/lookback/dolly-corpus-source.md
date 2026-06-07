# Lookback corpus source — databricks-dolly-15k

The second Tier-1 reference bundle (after WikiText-103, see `corpus-source.md`) seals
the **databricks-dolly-15k** instruction-tuning corpus. This note records exactly which
bytes are sealed so that anyone can reproduce the build and verify they started from the
same input. **The corpus data itself is never committed to this repository** — only this
provenance record is.

## Source

| Field | Value |
|---|---|
| Hugging Face dataset | [`databricks/databricks-dolly-15k`](https://huggingface.co/datasets/databricks/databricks-dolly-15k) |
| Config | `default` |
| Split | `train` |
| Format | Apache Parquet, columns `instruction`, `context`, `response`, `category` (all `string`) |
| License | `CC-BY-SA-3.0` (Databricks) |

dolly-15k is ~15k human-written instruction examples. Each row has an `instruction`, an
optional `context` paragraph, a `response`, and a free-text `category` tag. **One row =
one sealed leaf, rendered to natural text**: `instruction`, then the `context` block only
when non-empty, then `response`, each separated by a single blank line and ending in one
newline. The `category` tag is metadata, not training text, so it is **not** sealed
(founder decision, 2026-06-06). Unlike WikiText-103-raw the source is already natural
English, so there is **no detokenization step** — the seal renders and seals the source
text directly. See `crates/attestrum-pipeline/examples/dolly/render.rs` and
`docs/diagrams/lookback/dolly-seal-pipeline.md`. The render never touches the protected
fingerprint normalization (CLAUDE.md §4).

**Attribution.** Because the corpus is released under CC-BY-SA-3.0, the publish path
renders a source / attribution / modification / ShareAlike section on the dataset card
from [`dolly-attribution.md`](./dolly-attribution.md) (passed verbatim to
`attestrum publish --attribution-file`), satisfying the source license's §4 terms.

## Shards (train split — 1 file, ~7.4 MB total)

Resolved from the dataset's auto-converted Parquet branch
(`refs/convert/parquet`):

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `default/train/0000.parquet` | `7747823` | `51c85bb925785765bad9dd961e9db144b555cc79d58803242c9bf883375b938b` |

The SHA-256 equals the file's Hugging Face Git-LFS object id (the `lfs.oid` reported by
the HF tree API), so a re-fetch can be verified against Hugging Face without trusting this
repository.

URL:

- `https://huggingface.co/datasets/databricks/databricks-dolly-15k/resolve/refs%2Fconvert%2Fparquet/default/train/0000.parquet`

> Note: `refs/convert/parquet` is Hugging Face's auto-generated Parquet mirror of the
> dataset. The bytes are pinned by the SHA-256 above; if HF ever regenerates the mirror
> with different bytes, the hash here is the source of truth for what was sealed.

## Local layout (gitignored — never committed)

The shard lives under a repo-local working directory that the `.gitignore` tier-3 `/_*`
glob (CLAUDE.md §0.5.5) already excludes:

```
_lookback-data/databricks-dolly-15k/train/0000.parquet
```

## Re-fetch + verify

```bash
mkdir -p _lookback-data/databricks-dolly-15k/train
cd _lookback-data/databricks-dolly-15k/train
url="https://huggingface.co/datasets/databricks/databricks-dolly-15k/resolve/refs%2Fconvert%2Fparquet/default/train/0000.parquet"
curl -sSL -o 0000.parquet "$url"
shasum -a 256 0000.parquet   # must match the table above
```

The sealed corpus is then produced by the seal generator:

```bash
cargo run -p attestrum-pipeline --release --example seal-dolly \
  -- _lookback-data/databricks-dolly-15k/train _lookback-dolly-out
```

## Canonical seal (input → output, closeable)

Sealing the pinned input shard above with the seal generator yields this canonical
result. A verifier who re-runs the command on the byte-identical input must reproduce the
same Merkle root.

| Field | Value |
|---|---|
| Merkle root (BLAKE3, RFC 6962) | `58edb2fa39c9362306b2b10744f3f74115bc35a92456c126b38154cb0b35c6c7` |
| Leaves (rendered rows) | 15,011 |
| Total sealed bytes | 11,832,522 |
| `manifest.parquet` SHA-256 | `d965ea0829d87ec71127760360fbd127b3275fdc6b406aa450f6d7ce09c0395c` |
| Sealed by | `attestrum-pipeline` example `seal-dolly` (release) |

The root is printed to stdout by the seal command; the manifest lands at
`<output-dir>/.attestrum/manifests/manifest.parquet`. This seal was produced locally on
macOS/arm64; the `dolly-seal-crosscheck` workflow re-runs it on Linux x86_64/glibc (the
signing platform) and asserts the identical root, `manifest.parquet` SHA-256, and leaf
count before any signing phase — the precondition the `lookback-seal-crosscheck` workflow
established for WikiText-103.
