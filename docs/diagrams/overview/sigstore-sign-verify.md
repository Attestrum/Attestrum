---
title: "Sigstore sign-and-verify sequence — Bundle v0.3"
models: "Sigstore Bundle v0.3 / in-toto Statement v1 / Fulcio / Rekor v2 (external specs); crates/attestrum-attest/src/sign.rs, crates/attestrum-attest/src/verify.rs"
source_of_truth: spec
last_verified: bootstrap 2026-05-24
diagram_type: sequenceDiagram
---

# Sigstore sign + verify

Source of truth is the external Sigstore Bundle v0.3 / in-toto v1 / Fulcio / Rekor v2 specifications. We target `application/vnd.dev.sigstore.bundle.v0.3+json` exclusively (cosign v3's `--new-bundle-format`, which is v3's default). v0.1 / v0.3 bundle formats are not supported. Drift between this diagram and our emitted bundles means our implementation is wrong, not the spec.

```mermaid
sequenceDiagram
  autonumber
  participant U as User CLI<br/>(attestrum sign)
  participant A as attestrum-attest
  participant I as in-toto Statement<br/>v1
  participant D as DSSE envelope
  participant O as OIDC IdP<br/>(GitHub / Google / Microsoft)
  participant F as Fulcio CA
  participant R as Rekor v2 (tile-backed)
  participant V as Verifier<br/>(any third party, cosign v3+)

  U->>A: sign(manifest.parquet, predicate)
  A->>I: build Statement{_type, subject[], predicateType, predicate}
  I->>D: payload = base64(JSON Statement)<br/>payloadType=application/vnd.in-toto+json
  U->>O: request OIDC id_token (interactive or workload)
  O-->>U: id_token (JWT)
  U->>F: CSR + id_token
  F-->>U: short-lived X.509 cert (ephemeral key)
  U->>D: DSSE-sign payload with ephemeral key
  D->>R: submit { dsseEnvelope, verificationMaterial }
  R-->>D: signed inclusion proof + RFC3161 timestamp
  D->>A: assemble Bundle v0.3 JSON
  A-->>U: write bundle.sigstore.json

  Note over V: any time later, no Attestrum install needed
  V->>V: cosign verify-blob-attestation --new-bundle-format<br/>--bundle bundle.sigstore.json<br/>--certificate-identity-regexp ...<br/>--certificate-oidc-issuer ... manifest.parquet
  V-->>V: Verified OK
```
