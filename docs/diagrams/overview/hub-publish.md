---
title: "Hugging Face Hub publish flow"
models: "crates/attestrum-publish/src/lib.rs, crates/attestrum-emit/src/lib.rs, crates/attestrum-emit/src/croissant.rs, crates/attestrum-emit/src/cyclonedx.rs, crates/attestrum-emit/src/dataset_card.rs, crates/attestrum-emit/src/verify_html.rs, PublishTarget, HuggingFaceTarget, GitHubReleaseTarget, StaticBundleTarget, PublishPlan, PublishReceipt, AttestrumPublishError, render_croissant, render_cyclonedx, render_readme, render_verify_html_stub, CroissantPlan, CycloneDxPlan, DatasetCardPlan, VerifyHtmlPlan, ManifestStats, AttestrumEmitError"
source_of_truth: code
last_verified: 37ca5e1 2026-06-05
diagram_type: sequenceDiagram
---

# Hugging Face Hub publish

Source of truth flips to `code` at end of Sprint 6. The HF Hub does not expose a native Sigstore-bundle attestation endpoint for datasets (verified May 2026); the bundle is committed as a regular repo file via the standard `create_commit` API, exactly as the OpenSSF model-signing project and Cohere do today for models. We therefore make `croissant.json` an explicit repo-root file (mirroring established practice such as `huggingface.co/datasets/princeton-nlp/CharXiv/blob/main/croissant.json`), independent of the Hub-generated one.

```mermaid
sequenceDiagram
  autonumber
  participant U as User CLI<br/>(attestrum publish --target huggingface)
  participant P as attestrum-publish
  participant E as attestrum-emit
  participant H as huggingface.co Hub API
  participant V as verify.html<br/>(static page)

  U->>P: publish --dataset org/name --bundle bundle.sigstore.json
  P->>E: generate croissant.json<br/>(schema.org/Dataset + Attestrum provenance fields)
  E-->>P: croissant.json
  P->>E: generate cyclonedx.json<br/>(CycloneDX 1.6 ML-BOM: SHA-256 in hashes, BLAKE3 in properties)
  E-->>P: cyclonedx.json
  P->>E: generate README.md<br/>(YAML frontmatter + provenance section)
  E-->>P: README.md
  P->>E: generate verify.html<br/>(static, no deps)
  E-->>P: verify.html
  P->>H: POST /api/repos/create {type=dataset, name=org/name, exist_ok=true}
  H-->>P: 200 repo url
  P->>H: create_commit(operations=[<br/>  add(README.md),<br/>  add(croissant.json),<br/>  add(cyclonedx.json),<br/>  add(attestrum/manifest.parquet),<br/>  add(attestrum/merkle.root),<br/>  add(attestrum/bundle.sigstore.json),<br/>  add(attestrum/verify.html)<br/>])
  H-->>P: commit oid
  P-->>U: dataset URL + verification URL

  Note over V: any visitor, no install
  V->>V: fetch bundle + manifest<br/>verify with embedded WASM cosign-lite
  V-->>V: green check or red X
```
