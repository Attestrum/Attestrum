---
title: "build → sign → publish CI dry-run flow"
models: ".github/workflows/build-sign-publish.yml, .github/workflows/cosign-interop.yml, tests/fixtures/ci-publish-corpus/corpus.toml"
source_of_truth: code
last_verified: 7db9838 2026-06-12
diagram_type: flowchart
---

# build → sign → publish — CI dry-run

The `.github/workflows/build-sign-publish.yml` workflow runs the full corpus-to-bundle
pipeline **inside GitHub Actions** and proves a third party can verify the result with
`cosign` alone. It is the first workflow that runs `attestrum publish`; the existing
`cosign-interop.yml` only signs a synthetic empty-corpus manifest.

`source_of_truth: code` — the YAML at `.github/workflows/build-sign-publish.yml` is now
authoritative and this diagram is a derived view (roadmap v0.2a, §2 diagram-first).

**Identity split (the load-bearing point).** The provenance identity is the **ambient GHA
keyless OIDC** identity (`permissions: id-token: write`), exchanged to a `sigstore`-audience
token exactly as `cosign-interop.yml` does, and consumed by `attestrum sign` via
`SIGSTORE_ID_TOKEN`. It is **never** a personal identity (roadmap §21.1). Hugging Face Hub
has no GitHub-OIDC federation, so a real HF push needs a static token — out of scope here:
the dry-run uses `publish --target static` (local write, no network, no secret). The cosign
verify checks the **bundle**, which `sign` produces *before* `publish`, so the static target
fully exercises the §21.1 identity proof.

**Trigger = `workflow_dispatch`** (manual), not `push:main`: every signed run leaves a
permanent public Rekor transparency-log entry, so the cadence is capped to deliberate runs.

**Determinism (§7):** `SOURCE_DATE_EPOCH` is derived from the commit time
(`git show -s --format=%ct HEAD`), never wall-clock, and threaded identically through build,
sign, and publish.

```mermaid
flowchart TD
  D["workflow_dispatch<br/>(manual trigger)"] --> CO["actions/checkout@v4"]
  CO --> RT["dtolnay/rust-toolchain@1.89.0"]
  RT --> CA["actions/cache@v4<br/>(registry + git + target)"]
  CA --> BUILD_BIN["cargo build --release -p attestrum-cli<br/>(bin: attestrum)"]
  BUILD_BIN --> SDE["SOURCE_DATE_EPOCH = git show -s --format=%ct HEAD<br/>→ GITHUB_ENV (no wall-clock, §7)"]

  SDE --> BUILD["attestrum build<br/>--corpus tests/fixtures/ci-publish-corpus/corpus.toml<br/>--source-date-epoch $SOURCE_DATE_EPOCH"]
  BUILD --> M["manifest.parquet + merkle.root<br/>(workspace/.attestrum/manifests/)"]

  M --> OIDC["GHA OIDC exchange<br/>curl ACTIONS_ID_TOKEN_REQUEST_URL&audience=sigstore<br/>→ mask → SIGSTORE_ID_TOKEN"]
  OIDC --> SIGN["attestrum sign manifest.parquet<br/>--source-date-epoch $SOURCE_DATE_EPOCH"]

  SIGN --> FULCIO["Fulcio: ephemeral cert bound to<br/>GHA workflow SAN"]
  SIGN --> REKOR["Rekor v2: transparency-log entry"]
  FULCIO --> BUNDLE["bundle.sigstore.json<br/>(workspace/bundles/)"]
  REKOR --> BUNDLE

  BUNDLE --> PUB["attestrum publish --target static<br/>--manifest ... --bundle ... --out-dir static-out"]
  PUB --> ARTIFACTS["static-out/<br/>README.md, croissant.json, cyclonedx.json,<br/>attestrum/{manifest.parquet, merkle.root,<br/>bundle.sigstore.json, verify.html}"]

  BUNDLE --> CI_INSTALL["sigstore/cosign-installer@v3<br/>+ cosign version >= v2.5 guard"]
  CI_INSTALL --> VERIFY{"cosign verify-blob-attestation<br/>--new-bundle-format<br/>--type …/training-corpus/v0.3<br/>--certificate-identity-regexp = Attestrum GHA SAN<br/>--certificate-oidc-issuer = token.actions.githubusercontent.com"}
  VERIFY -->|"Verified OK<br/>+ SAN is the workflow, NOT a person"| PASS["job green"]
  VERIFY -->|"mismatch / personal SAN"| FAIL["job red"]
```
