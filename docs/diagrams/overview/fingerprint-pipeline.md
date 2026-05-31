---
title: "fingerprint generation pipeline"
models: "crates/attestrum-fingerprint/src/lib.rs, crates/attestrum-fingerprint/src/text.rs, crates/attestrum-fingerprint/src/image.rs, crates/attestrum-fingerprint/src/audio.rs, crates/attestrum-fingerprint/src/video.rs"
source_of_truth: diagram
last_verified: 304757a 2026-05-30
diagram_type: flowchart
---

# Fingerprint pipeline

Source of truth flips to `code` when `crates/attestrum-fingerprint/src/lib.rs` lands in Sprint 5. ISO 24138:2024 ISCC is implemented via the official `iscc-lib` Rust crate (the only ISO 24138:2024 conformance-tested polyglot library, with Rust at its core and Python/Java/Go bindings). Image perceptual hashes use the `image_hasher` crate (the maintained fork of `img_hash`, MSRV 1.70). Text MinHash/SimHash are implemented in-tree because public crates are either toy implementations or unmaintained.

```mermaid
flowchart TD
  IN[document bytes] --> MOD[modality detection<br/>magic bytes + extension]
  MOD --> B[BLAKE3 stream hash<br/>always]
  MOD --> R{modality}
  R -->|text/plain text/* application/pdf| T[text branch]
  R -->|image/*| I[image branch]
  R -->|audio/*| A[audio branch]
  R -->|video/*| V[video branch]
  R -->|other| X[skip non-BLAKE3]
  T --> T1[normalized tokenization<br/>NFC + lowercase + collapse-ws]
  T1 --> T2[5-gram shingles]
  T2 --> T3[MinHash 128]
  T2 --> T4[SimHash 64]
  T --> ISC1[ISCC text-code<br/>iscc-lib]
  I --> ISC2[ISCC image-code]
  I --> P1[pHash 64 / dHash 64 / aHash 64<br/>image_hasher crate]
  I --> P2[blockhash 64]
  A --> ISC3[ISCC audio-code]
  A --> CH[chromaprint or audfprint<br/>FFI]
  V --> ISC4[ISCC video-code]
  V --> KF[keyframe pHash sequence]
  B --> BUN[FingerprintBundle JSON]
  ISC1 --> BUN
  ISC2 --> BUN
  ISC3 --> BUN
  ISC4 --> BUN
  T3 --> BUN
  T4 --> BUN
  P1 --> BUN
  P2 --> BUN
  CH --> BUN
  KF --> BUN
  X --> BUN
```
