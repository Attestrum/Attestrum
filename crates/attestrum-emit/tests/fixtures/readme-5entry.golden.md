---
dataset_name: "my-org/my-dataset"
language: ["en"]
license: "Apache-2.0"
pretty_name: "My Dataset (v0.1)"
size_categories: ["n<1K"]
task_categories: ["text-generation"]
tags:
  - "example"
  - "attestrum-provenance"
  - "sigstore-signed"
  - "croissant"
attestrum:
  bundle: "attestrum/bundle.sigstore.json"
  manifest: "attestrum/manifest.parquet"
  merkle_root: "attestrum/merkle.root"
  predicate: "https://attestrum.com/attestation/training-corpus/v0.3"
  verify_url: "./attestrum/verify.html"
---

# My Dataset (v0.1)

This dataset's provenance is cryptographically verifiable. The corpus's training-time content is described by a sealed Merkle-rooted manifest signed with Sigstore. The signing identity is recorded in a Rekor transparency-log entry; anyone can verify the chain end-to-end without Attestrum installed.

## Verification

- Hosted verify page: [https://huggingface.co/datasets/my-org/my-dataset/blob/main/attestrum/verify.html](https://huggingface.co/datasets/my-org/my-dataset/blob/main/attestrum/verify.html)
- CLI: `cosign verify-blob-attestation --new-bundle-format --bundle attestrum/bundle.sigstore.json attestrum/manifest.parquet`

## Corpus stats

- Documents: 5
- Total bytes: 1024

## Attestrum metadata

The provenance descriptor (Croissant JSON-LD) lives at `croissant.json`; the sealed manifest at `attestrum/manifest.parquet`; the Merkle root at `attestrum/merkle.root`; the Sigstore bundle at `attestrum/bundle.sigstore.json`. The signing predicate is `https://attestrum.com/attestation/training-corpus/v0.3`.
