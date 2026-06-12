# Lookback corpus source — deepmind-pg19

The third Tier-1 reference bundle (after WikiText-103, `corpus-source.md`, and
dolly-15k, `dolly-corpus-source.md`) seals **PG-19**, DeepMind's Project Gutenberg
books corpus: 28,752 complete books published before 1919, ~11.5 GB of plain text.
This note records exactly which bytes are sealed so that anyone can reproduce the
build and verify they started from the same input. **The corpus data itself is never
committed to this repository** — only this provenance record is.

PG-19 is the first rung sealed entirely **in CI**: the corpus exceeds what a laptop
seals in reasonable time, and a stock runner's disk only fits it after a cleanup
step. The `pg19-seal-crosscheck` workflow downloads, seals, and asserts the
canonical values below; the `pg19-publish` workflow re-runs that and signs only if
the values reproduce.

## Source

| Field | Value |
|---|---|
| Hugging Face dataset | [`deepmind/pg19`](https://huggingface.co/datasets/deepmind/pg19) |
| HF revision (pinned) | `4d28bd77e66947ad3835cf78ed7aaeb4dd87ad8b` |
| Data host | `https://storage.googleapis.com/deepmind-gutenberg/` (DeepMind's public GCS bucket — the HF repo carries only the loader script and file lists) |
| Format | One plain-text UTF-8 file per book: `train/` 28,602 files, `validation/` 50, `test/` 100; max single file ~4.5 MB |
| License | `Apache-2.0` (dataset compilation, DeepMind); book content is pre-1919 Project Gutenberg public domain |

**One book file = one sealed leaf, exact bytes.** There is no rendering or
normalization step of any kind: the sealed bytes are the file bytes as distributed,
so every leaf digest in the published manifest can be checked directly against the
upstream file (`b3sum train/10005.txt`). All three splits are sealed — the corpus is
the whole dataset. Entry order is lexicographic by relative path; the `source_uri`
backref is `pg19://<relative-path>`. See
`docs/diagrams/lookback/pg19-seal-pipeline.md`. The seal never touches the protected
fingerprint normalization (CLAUDE.md §4).

DeepMind's published preprocessing (disclosed upstream, not ours): Project Gutenberg
license boilerplate was stripped from the book texts, and certain words were
replaced with `<DW>` tokens. We seal the files exactly as DeepMind distributes them.

**Attribution.** The publish path renders a source / attribution / modification
section on the dataset card from [`pg19-attribution.md`](./pg19-attribution.md)
(passed verbatim to `attestrum publish --attribution-file`).

## Pinned index files

The dataset's own file lists enumerate all 28,752 book paths and are the source of
the download URL set. Pinned at the HF revision above:

| File | Size (bytes) | Lines | SHA-256 |
|---|---|---|---|
| `data/train_files.txt` | `451147` | 28,602 | `145b5f0896cc2fff3231222874a0f324ee04559144ecf25a873d294eb161719c` |
| `data/validation_files.txt` | `1036` | 50 | `e66d9f76a39a7a73a62270e98395c9532d306ecd2b7a10ae4098dcdd133d179b` |
| `data/test_files.txt` | `1475` | 100 | `c84c08139695f3312df83239a1a41e7b9cde1baf7c08bfcf230ae09eaaf18d8c` |
| `metadata.csv` (GCS root; id,title,year,url — index only, **not sealed**) | `2737447` | 28,752 | `fbb2fdb48522927b2e16aa52950f2afeb83c6fa8fed45f0c3dd834e9bc9b43b9` |

The 28,752 book files themselves carry no upstream-published digests; their
per-file BLAKE3 + SHA-256 digests are exactly what the sealed `manifest.parquet`
records, and the canonical Merkle root below pins the complete set. Any upstream
byte drift fails the crosscheck assertion before anything is signed.

## Local layout (gitignored — never committed)

```
_lookback-data/deepmind-pg19/train/<id>.txt        (28,602 files)
_lookback-data/deepmind-pg19/validation/<id>.txt   (50 files)
_lookback-data/deepmind-pg19/test/<id>.txt         (100 files)
```

Covered by the `.gitignore` tier-3 `/_*` glob (CLAUDE.md §0.5.5).

## Re-fetch + verify

```bash
rev=4d28bd77e66947ad3835cf78ed7aaeb4dd87ad8b
base=https://huggingface.co/datasets/deepmind/pg19/resolve/$rev/data
mkdir -p _lookback-data/deepmind-pg19 && cd _lookback-data/deepmind-pg19
for f in train_files.txt validation_files.txt test_files.txt; do
  curl -sSL -o "$f" "$base/$f"
done
shasum -a 256 *_files.txt          # must match the table above
cat *_files.txt \
  | sed 's#^#https://storage.googleapis.com/deepmind-gutenberg/#' > urls.txt
aria2c -j16 -x4 --max-tries=5 --retry-wait=5 --auto-file-renaming=false \
  --dir . --input-file urls.txt    # ~11.5 GB; paths mirror train/... etc.
```

The sealed corpus is then produced by the seal generator:

```bash
cargo run -p attestrum-pipeline --release --example seal-pg19 \
  -- _lookback-data/deepmind-pg19 _lookback-pg19-out
```

## Canonical seal (input → output, closeable)

**Pending capture.** The canonical values are produced by the first
`pg19-seal-crosscheck` run in `mode=capture` on Linux x86_64/glibc (the signing
platform) and pinned here plus in the workflow's `CANONICAL_*` env in the same
commit. Until then this section is intentionally blank — nothing is signed before
a second run in `mode=assert` reproduces the triple byte-for-byte.

| Field | Value |
|---|---|
| Merkle root (BLAKE3, RFC 6962) | _pending capture_ |
| Leaves (book files) | _pending capture — expected 28,752_ |
| Total sealed bytes | _pending capture_ |
| `manifest.parquet` SHA-256 | _pending capture_ |
| Sealed by | `attestrum-pipeline` example `seal-pg19` (release, CI) |

## Scale evidence

**Pending capture.** PG-19 is the ladder's calibration rung; the capture run
records download throughput (GCS → runner), seal wall-time, and peak disk here.
