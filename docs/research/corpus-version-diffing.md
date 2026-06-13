# Corpus-Version Diffing: A Byte-Reproducible Delta Between Two Sealed Corpus States

Engineering documentation / feature explanation. Describes `attestrum diff`, the
read-only subcommand that compares two already-sealed corpus manifests and emits a
deterministic delta report — *what changed between version N and version N+1 of a
training mix, in a report anyone can re-check byte-for-byte.* Intended for engineers,
auditors, and partners evaluating how Attestrum reasons about corpus **evolution**
(across versions), as opposed to a single sealed corpus.

Companion to [`how-attestrum-works-end-to-end.md`](./how-attestrum-works-end-to-end.md)
(the single-corpus seal → sign → publish pipeline this builds on) and
[`deterministic-by-construction.md`](./deterministic-by-construction.md) (the
byte-identity disciplines the diff's determinism promise rests on).

Status: applies to the `attestrum-diff` crate and the `attestrum diff` subcommand as
landed. The worked example in §3 is genuine, unedited tool output.

---

## TL;DR

Curation pipelines *transform* a corpus; data-version-control systems *store* its
successive states. Neither cleanly answers a question every data team eventually asks:
*what exactly changed between two corpus versions — and can I prove it?* `attestrum diff
<old> <new>` compares two already-sealed corpus manifests and reports documents
**added / removed / unchanged**, **multiset (occurrence-count) shifts**, and
**composition shift** across five lenses (modality, source type, source dataset, SPDX
license, language), plus each version's BLAKE3 Merkle root — as a byte-reproducible
`report.json` and a human-readable summary. Because both endpoints are cryptographically
sealed, the delta is not a description you have to trust; it is evidence anyone can
recompute. Content identity is the BLAKE3 content hash, so an *edited* document is
reported honestly as one `removed` + one `added`, never a fuzzy "modified" verdict — and
the report declares its own current limits in-band.

## 1. What it is

`attestrum diff <old> <new>` is a deterministic, read-only diff between two already-sealed
Attestrum corpus manifests. It has no write path, signs nothing, and mutates nothing — it
reads two `manifest.parquet` files and reports what changed between them.

A single run reports:

- **Documents added / removed / unchanged**, by content identity.
- **Multiset (occurrence-count) shifts** — a document that appeared 3× in the old corpus
  but 1× in the new one is unchanged in identity yet shifted in multiplicity, and the
  report records that explicitly.
- **Composition shift across five lenses** — modality, source type, source dataset, SPDX
  license, and language — each expressed as the old share and new share of the corpus,
  over the union of labels present in either version.
- **Per-version summaries** — each version's BLAKE3 Merkle root (the cryptographic name
  of that endpoint), total document count, distinct-document count, exact-duplicate count,
  and total bytes.

Output comes in two surfaces: a byte-reproducible `report.json` (full detail, canonical
key ordering) and a concise human summary on stdout. Internally the comparison is a
streaming merge-join over two canonically sorted manifest streams, so it never
materializes either manifest in full — peak memory is the per-side leaf-digest vectors
plus one batch per reader, the same envelope `attestrum merge` accepts. It scales to
manifests with tens of millions of rows.

## 2. Why it is valuable

The honest starting state for most corpus work is a hunch: *"I think the dedup pass
dropped some short documents,"* or *"I'm fairly sure we rebalanced toward web text."*
`attestrum diff` turns that hunch into an auditable, byte-reproducible delta. Both
endpoints are already sealed — each carries its own BLAKE3 Merkle root — so the delta
between them is verifiable evidence, not a vibe. Anyone with the two manifests can
recompute the identical report and get the identical bytes.

This fills a real gap in the tooling landscape. Data-curation frameworks **transform** a
corpus — they apply filters, dedup, and rebalancing, and the transformation is the
product. Data-version-control systems like [DVC](https://dvc.org/doc/command-reference/diff)
and [lakeFS](https://lakefs.io/data-version-control/) **store** successive states and give
Git-like branch/commit/diff operations over files and objects. Both are valuable, and
Attestrum replaces neither. But neither cleanly answers *"what exactly changed between
these two corpus states, in a report I can re-check byte-for-byte and stand behind as
evidence?"* File-level data diffs tell you which blobs moved; they do not give you a
content-addressed, composition-aware, independently reproducible delta tied to a
cryptographic root on each side. That is the specific question `attestrum diff` answers.

There is also a compliance dimension. The EU AI Act's Article 53(1)(d) obliges
general-purpose AI model providers to publish a [sufficiently detailed summary of training
content](https://www.wilmerhale.com/en/insights/blogs/wilmerhale-privacy-and-cybersecurity-law/european-commission-releases-mandatory-template-for-public-disclosure-of-ai-training-data),
including the general characteristics and the datasets and sources used. When a published
corpus is revised, *"here is exactly how the composition changed from the previous
version"* is a question such a summary may need to answer — and a per-source, per-license,
per-language share shift that recomputes byte-for-byte is a clean way to back it.

One caveat, stated up front: `attestrum diff` **measures** a change. It does not judge
whether the change improved the data. A share shift toward synthetic text or away from a
particular license is reported neutrally; the interpretation is yours.

## 3. How to use it

The command takes two sealed manifests, old then new:

```
attestrum diff <old.parquet> <new.parquet> [--out <report.json>] [--timestamp <string>]
```

- **`--out <path>`** writes the full deterministic `report.json`. Omit it for just the
  stdout summary.
- **`--timestamp <string>`** embeds a Reproducible-Builds-style timestamp into the report
  verbatim. Absent it, the report carries **no wall-clock at all** — which is what keeps
  the output byte-identical across runs and machines.

Exit codes mirror `attestrum inspect`, so the diff slots into the same scripting and CI
conventions as the rest of the CLI: `0` success; `2` argument error (a path is missing or
not a file); `8` schema-version mismatch (a manifest's schema version is unsupported);
`1` other read error.

### Worked example (genuine output)

Take two versions of a small corpus. **v1** = `{doc01, doc02, doc03}`. **v2** is the
result of a curation pass that *edited* `doc01`, *kept* `doc02`, *dropped* `doc03`, and
*added* `doc04`:

```
$ attestrum diff v1/manifest.parquet v2/manifest.parquet --out report.json
attestrum diff: wrote report.json
corpus diff (attestrum-diff-report/0.1)
identity mode: content-addressed multiset over BLAKE3 document_id; no "modified" category (a changed document is a removed old hash plus an added new hash)
old: fcdee42e76e055cb4090eec18919caef3475722dd31dc06b3c5e1d0c09a0dd92  (3 docs, 3 distinct, 645 bytes)
new: 60f9043815eea1a07dca6e53d77960a4e22a20daee5c98369e928b67ec0b94cd  (3 docs, 3 distinct, 406 bytes)
delta: added 2 · removed 2 · unchanged 1 · multiset-shifts 0
```

Read the counts carefully, because they teach the whole model. The delta is **added 2 /
removed 2 / unchanged 1** — not the "1 modified, 1 removed, 1 added" you might expect:

- **`doc01` was edited.** Its content changed, so its BLAKE3 content hash changed. The
  diff reports its *old* hash as one `removed` and its *new* hash as one `added`. There is
  no line connecting them — and that is deliberate.
- **`doc03` was dropped** → one more `removed`.
- **`doc04` was added** → one more `added`.
- **`doc02` is byte-for-byte identical** → the single `unchanged`.

The two distinct Merkle roots on the `old:` and `new:` lines name the two corpus states
cryptographically; the byte-count drop (645 → 406) reflects the smaller revised set.

## 4. When to use it

`attestrum diff` is built for the moments where a corpus changes and someone needs to know
exactly how:

- **After a curation pass** — dedup, filtering, or source rebalancing — to see precisely
  what came out and what the composition shift was.
- **When re-cutting a training mixture** and you need the delta between the old mix and the
  new one.
- **In CI**, to gate or document corpus changes: diff the previous sealed manifest against
  the new one, and fail the build or attach the report when the delta crosses a threshold
  you care about.
- **When auditing what a data pipeline actually did** versus what it was supposed to do —
  the report is ground truth, independent of the pipeline's own logs.
- **When comparing two published versions of a dataset**, to produce a reproducible,
  evidence-grade statement of how they differ.

## 5. Why we made it — design philosophy

**The honesty model is the centerpiece.** A manifest's `document_id` *is* the BLAKE3
content hash of the document. That single decision dictates everything: content identity
is content hash, full stop. So an *edited* document is reported honestly as one `removed`
(its old hash) plus one `added` (its new hash) — **never** a fuzzy-matched "modified"
verdict. There is no caller-stable id in the manifest schema that could link an old
version of a document to a new one, so the tool does not pretend there is. It states its
identity mode in every report rather than guessing at a relationship it cannot prove.

**The report declares its own limits, in-band.** Rather than leave you to discover the
edges, every report carries an explicit list of what it deliberately does *not* do in this
version:

- **No "modified" category** — that would require a caller-stable document id, a change to
  the protected manifest schema.
- **No near-duplicate-rate delta** — per-document fingerprints are not persisted in the
  manifest, so a fuzzy near-duplicate shift is out of scope for this subcommand.
- **No signed corpus-delta artifact yet** — the diff is the reversible, unsigned *leaf*,
  deliberately not a frozen, signed predicate.

**Determinism is the one absolute promise, and it is CI-enforced.** Same two manifests →
byte-identical report on any machine. The merge-join walks in `document_id` order, every
accumulator is a `BTreeMap`, every example list is sorted by construction, shares round to
six decimals for stable float formatting, and JSON rendering rides a shared canonical-key
primitive. No wall-clock unless you supply one. A committed golden plus a double-run
byte-compare gate this in CI on every push.

That is the whole intent: a small, single-purpose instrument whose output is meant to
stand as evidence. It tells you exactly what changed between two sealed corpus states,
declares exactly what it can and cannot claim, and produces the same bytes every time.
Precision over breadth.

---

### Sources

- [DVC — `diff` command reference](https://dvc.org/doc/command-reference/diff)
- [lakeFS — Data Version Control](https://lakefs.io/data-version-control/)
- [WilmerHale — European Commission Releases Mandatory Template for Public Disclosure of AI Training Data](https://www.wilmerhale.com/en/insights/blogs/wilmerhale-privacy-and-cybersecurity-law/european-commission-releases-mandatory-template-for-public-disclosure-of-ai-training-data)
