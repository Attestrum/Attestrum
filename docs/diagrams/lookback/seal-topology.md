---
title: "Lookback Phase A — seal-local, sign-in-cloud, publish-to-HF topology"
models: "crates/attestrum-pipeline/src/lib.rs, crates/attestrum-attest/src/sign.rs, crates/attestrum-publish/src/lib.rs, .github/workflows/build-sign-publish.yml"
source_of_truth: diagram
last_verified: 4226bba 2026-05-31
diagram_type: flowchart
---

# Lookback Phase A — seal/sign/publish topology (planning)

**Source of truth: `diagram`** — forward-looking design for where each step of sealing the whole WikiText‑103 runs. Flips to `code` once the Phase‑A workflow lands.

Two invariants drive the layout:

1. **The Sigstore signature is always applied by the Attestrum GitHub Actions keyless identity, never a personal one** (CLAUDE‑LOCAL §A9). `attestrum sign` consumes whatever OIDC token is present, so run locally it would sign under the founder's personal identity — forbidden. Signing therefore happens inside GitHub Actions, where the ambient GHA OIDC token resolves to the Attestrum workflow SAN.
2. **The heavy build runs where there's RAM.** Phase 0 measures the ~1M-leaf build's peak memory. If it fits a free GitHub-hosted runner (~7 GB) the whole pipeline runs in CI (cleanest — fully reproducible from the workflow alone). If not, the founder's preference is to seal locally (16 GB box) and hand only the `manifest.parquet` to CI to sign — the manifest is all `sign` needs (it signs the manifest bytes, not the CAS).

The diamond is the Phase‑0 decision; both branches converge on the same in‑CI sign + publish.

```mermaid
flowchart TD
  D{"~1M-leaf build fits<br/>free CI runner?<br/>(Phase 0 measures)"}

  subgraph LOCAL["Local machine (16 GB)"]
    GENL["seal generator<br/>passages &rarr; build_corpus"]
    MANL["manifest.parquet + merkle.root"]
    GENL --> MANL
  end

  subgraph GHA["GitHub Actions — Attestrum keyless identity"]
    GENC["seal generator (in CI)"]
    MANC["manifest.parquet + merkle.root"]
    OIDC["ambient GHA OIDC token<br/>(audience: sigstore)"]
    FULCIO["Fulcio &rarr; ephemeral cert<br/>(Attestrum workflow SAN)"]
    SIGNJ["attestrum sign manifest.parquet"]
    REKOR["Rekor transparency-log entry"]
    PUB["attestrum publish --target huggingface<br/>(HF_TOKEN secret)"]
    GENC --> MANC
    OIDC --> FULCIO --> SIGNJ
    MANC --> SIGNJ
    SIGNJ --> REKOR
    SIGNJ --> BUN["bundle.sigstore.json"]
    BUN --> PUB
    MANC --> PUB
  end

  subgraph PUBLIC["Public"]
    HF["Hugging Face<br/>Attestrum/wikitext-103-sealed"]
    TP["third party:<br/>cosign verify-blob-attestation<br/>(no Attestrum) &rarr; Verified OK"]
  end

  D -- "yes: all-in-CI" --> GENC
  D -- "no: seal local" --> GENL
  MANL -. manifest handoff<br/>(mechanism: Phase 0) .-> MANC
  PUB --> HF
  HF --> TP

  classDef dec fill:#5f4a1f,stroke:#e0a52e,color:#fff
  classDef local fill:#1f3a5f,stroke:#4a90d9,color:#fff
  classDef ci fill:#1f5f3a,stroke:#3ec072,color:#fff
  classDef pub fill:#3a2f5f,stroke:#9a7ad9,color:#fff
  class D dec
  class GENL,MANL local
  class GENC,MANC,OIDC,FULCIO,SIGNJ,REKOR,PUB,BUN ci
  class HF,TP pub
```

**Why the manifest-only handoff is sound:** `attestrum sign` (`crates/attestrum-attest/src/sign.rs`) signs the manifest's bytes/digest, not the content-addressed store. The CAS (~1M objects) never needs to leave the local machine for signing. Publishing to HF likewise uploads `manifest.parquet` + `bundle.sigstore.json` + the emitted Croissant/README/verify.html — not the CAS. The exact local→CI handoff mechanism (push unsigned manifest to the HF repo first, or a transient release asset/object-store hop) is the Phase‑0 deliverable.
