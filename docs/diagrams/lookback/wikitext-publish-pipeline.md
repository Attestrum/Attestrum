---
title: "Lookback Phase A — gated WikiText build→sign→publish pipeline"
models: ".github/workflows/lookback-publish.yml, crates/attestrum-pipeline/examples/seal-wikitext.rs, crates/attestrum-cli/src/commands/sign.rs, crates/attestrum-cli/src/commands/publish.rs"
source_of_truth: code
last_verified: b547f16 2026-06-12
diagram_type: sequenceDiagram
---

# Lookback Phase A — gated WikiText publish pipeline

The flagship publish flow, realised as `.github/workflows/lookback-publish.yml`
(`workflow_dispatch` only). It seals the **real** WikiText-103 corpus in CI, gates on
the canonical root before signing, signs keyless under Attestrum's GitHub Actions
identity (CLAUDE-LOCAL §A9), and publishes — either `static` (sign + local artifacts
for inspection) or `huggingface` (sign + Hub push). `source_of_truth: code` now that
`lookback-publish.yml` has landed and run — the WikiText-103 corpus is sealed, signed,
and published live; this diagram is the derived view, re-verify when the workflow changes.

**The ⛔ steps are signing (a permanent public Rekor entry) and the HF push (needs the
`HF_TOKEN` secret).** Both run only on an explicit manual dispatch. The pre-sign gate
guarantees only the canonical artifact (`de95bddc…`) is ever signed.

```mermaid
sequenceDiagram
    actor F as Founder
    participant GHA as GHA runner<br/>(lookback-publish.yml)
    participant HF as Hugging Face<br/>(pinned shards)
    participant FUL as Fulcio
    participant REK as Rekor<br/>(transparency log)
    participant HUB as HF Hub<br/>(dataset repo)
    participant COS as cosign

    F->>GHA: workflow_dispatch(target, dataset)
    GHA->>HF: download 2 pinned shards
    HF-->>GHA: 0000.parquet / 0001.parquet
    GHA->>GHA: SHA-256 precheck (pinned digests)
    GHA->>GHA: seal-wikitext -> manifest.parquet + merkle.root
    GHA->>GHA: pre-sign gate: root==de95bddc, manifest SHA, leaves==822559
    Note over GHA: abort here if the seal diverges —<br/>never sign a non-canonical artifact

    GHA->>FUL: GHA OIDC (audience=sigstore) -> CSR
    FUL-->>GHA: ephemeral cert (Attestrum workflow SAN)
    GHA->>REK: submit DSSE in-toto entry
    Note over REK: PERMANENT public entry (irreversible) ⛔
    REK-->>GHA: inclusion proof -> bundle.sigstore.json

    alt target = static (default)
        GHA->>GHA: render README + croissant + cyclonedx + verify.html
        GHA->>F: upload-artifact (inspect card/attribution before any push)
    else target = huggingface
        Note over GHA,HUB: needs HF_TOKEN secret ⛔
        GHA->>HUB: push 7 files (--attribution-file)
        HUB-->>GHA: commit_oid + dataset URL
    end

    GHA->>COS: verify-blob-attestation (assert Attestrum SAN)
    COS-->>GHA: Verified OK
```

**Pre-sign gate (step before any signing).** The same triple assertion as
`lookback-seal-crosscheck.yml` (root `de95bddc…`, `manifest.parquet` SHA-256
`eafa3dd7…`, leaves `822559`), moved *inside* the publish path so a divergent seal
aborts the run before Fulcio/Rekor are ever contacted.

**Signing identity.** Keyless via the GHA-OIDC→`audience=sigstore` exchange → Fulcio
ephemeral cert bound to the Attestrum workflow SAN, never a personal identity
(CLAUDE-LOCAL §A9). The closing `cosign verify-blob-attestation` step asserts that SAN
+ the GitHub OIDC issuer + the predicate type (`--type`), so a wrong identity or
predicate fails the run (the §21.1 guard, reused from `build-sign-publish.yml`).

**Determinism note.** The seal runs at epoch 0 (the canonical `de95bddc…` input); sign
and publish take a commit-derived `SOURCE_DATE_EPOCH` for reproducible predicate /
dataset-card timestamps. The two epochs are independent and both correct.
