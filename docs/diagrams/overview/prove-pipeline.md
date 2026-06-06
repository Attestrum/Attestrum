---
title: "attestrum prove pipeline — inclusion and non-inclusion"
models: "crates/attestrum-prove/src/lib.rs::prove, crates/attestrum-fingerprint, crates/attestrum-merkle, crates/attestrum-attest"
source_of_truth: diagram
last_verified: 8d49acc 2026-06-06
diagram_type: flowchart
---

# attestrum prove — pipeline

Source of truth flips to `code` at end of Sprint 5 when `attestrum-prove::prove` lands. The proof bundle is **separately signed** and references the corpus manifest by digest in its `subject[]` array. This separation is intentional: a corpus publisher may delegate proof issuance to a different identity (e.g., a hosted Attestrum service operated by Hugging Face) without compromising corpus authorship.

```mermaid
flowchart TD
  IN[attestrum prove DOC --against MANIFEST] --> PARSE{input kind?}
  PARSE -->|BLAKE3 hex| EX[exact-hash match]
  PARSE -->|ISCC URI| IS[ISCC similarity match]
  PARSE -->|perceptual hash| PH[perceptual distance match]
  PARSE -->|raw text / file| FP[fingerprint document<br/>attestrum-fingerprint]
  FP --> ROUTE{modality}
  ROUTE -->|text| TX[MinHash + SimHash<br/>n-gram shingles]
  ROUTE -->|image/audio/video| PH2[ISCC + perceptual]
  ROUTE -->|other| EX2[BLAKE3 only]
  EX --> LM[load manifest source]
  IS --> LM
  PH --> LM
  TX --> LM
  PH2 --> LM
  EX2 --> LM
  LM --> RES{resolve source}
  RES -->|local .parquet| LP[mmap Parquet]
  RES -->|hf://org/name| HF[HF Hub fetch]
  RES -->|https://registry/...| HT[registry fetch]
  LP --> Q[query index]
  HF --> Q
  HT --> Q
  Q --> M{match found?}
  M -->|yes, exact| AP[build Merkle audit path]
  M -->|yes, similar| AP2[build Merkle audit path<br/>+ similarity score]
  M -->|no| SN[build sorted-neighbor proof]
  AP --> PT1[InclusionProof predicate<br/>attestrum.com/attestation/inclusion-proof/v0.1]
  AP2 --> PT1
  SN --> PT2[NonInclusionProof predicate<br/>attestrum.com/attestation/non-inclusion-proof/v0.1]
  PT1 --> SIGN[sign DSSE envelope<br/>separate Sigstore bundle]
  PT2 --> SIGN
  SIGN --> OUT[proof.sigstore.json]
```
