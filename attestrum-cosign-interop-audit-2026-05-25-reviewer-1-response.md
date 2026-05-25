# Attestrum cosign-interop audit response

Date: 2026-05-25

## Recommendation

Do not choose Option Z. Do not shell out to cosign Go unless the Rust path proves blocked.

The best path is a modified Option Y:

1. Implement DSSE bundle signing inside the existing Attestrum sigstore-rs fork first.
2. Expose it as a small `sign_dsse` or `sign_attestation` API on `SigningSession`.
3. Use that API from `crates/attestrum-attest/src/sign.rs`.
4. Get Attestrum cosign-interop green.
5. Then upstream the sigstore-rs patch as a PR.

This is better than putting the DSSE construction directly inside Attestrum because the signing behavior belongs in the Sigstore SDK layer, not in the Attestrum product crate. You already have an approved fork for the workload-identity fix, so adding a third fork commit is the cleanest near-term move. It also preserves the long-term story: Attestrum is building on and improving open Sigstore infrastructure, not becoming a custom one-off signer.

I would treat this as a blocker before Sprint 5 API freeze. The bundle shape is a public artifact contract. Freezing APIs while the bundle shape is still wrong will create avoidable churn.

## My confidence in the uploaded diagnosis

High. I would raise the uploaded audit author’s confidence from about 85% to about 95%.

The key claim is correct:

Attestrum currently signs the SHA-256 hash of the canonical in-toto Statement JSON, but then tries to verify that same signature against the SHA-256 hash of the manifest Parquet file. Those are different byte streams, so the signature cannot verify.

The uploaded audit says the current bundle is a MessageSignature bundle over the Statement JSON hash, not a DSSE attestation bundle. Public upstream sigstore-rs source supports that conclusion. Current `SigningSession::sign` computes a SHA-256 digest, signs that digest with the ephemeral P-256 key, submits a `Hashedrekord` entry, and `SigningArtifact::to_bundle()` emits `Content::MessageSignature` with `Version::Bundle0_2`. It does not emit `Content::DsseEnvelope`.

The verify-side flow also supports the diagnosis. In upstream sigstore-rs, `verify_bundle_content` verifies `MessageSignature` by checking the signature against the provided input digest. For DSSE, it verifies the signature over PAE bytes and separately compares the in-toto subject SHA-256 digest against the provided artifact digest. That means Attestrum’s verifier only works if the caller passes the same input bytes that were signed. Right now signing receives Statement JSON, while verify receives the manifest file.

## External confirmation

The Sigstore bundle documentation says bundle content must be either a Message Signature over an artifact hash or a DSSE envelope for attestations. It specifically describes DSSE envelopes in Sigstore bundles as having `payloadType` set to `application/vnd.in-toto+json`, with the payload being an in-toto statement and only one signature.

That matches the project’s stated goal: `cosign verify-blob-attestation --new-bundle-format` should verify the emitted bundle without Attestrum installed. For that goal, the bundle must look like a cosign-compatible DSSE in-toto attestation, not a MessageSignature over a Statement JSON hash.

The sigstore-rs README also describes the crate as under active development and not stable before 1.0. That matters because relying on its current high-level `SigningSession::sign` behavior as if it already supports attestation signing was an unsafe assumption.

## What I would do next

### Step 1: Freeze the current diagnosis as accepted

Record this as the working root cause:

`attest_sign` signs canonical in-toto Statement JSON through sigstore-rs `SigningSession::sign`, which produces a Bundle v0.2 `MessageSignature` plus `Hashedrekord`. `attest_verify` then passes `manifest.parquet` to `Verifier::verify`, causing sigstore-rs to verify the signature against `SHA-256(manifest.parquet)`. Since the signed digest was `SHA-256(statement_json)`, verification fails with `Public key verification error`.

Also update the misleading comments in `crates/attestrum-attest/src/sign.rs`. The current comments describe DSSE behavior that does not exist.

### Step 2: Implement modified Option Y in the fork

Add a new API to the Attestrum sigstore-rs fork, probably one of these:

```rust
SigningSession::sign_dsse(payload_type: &str, payload: impl Read) -> SigstoreResult<SigningArtifact>
```

or:

```rust
SigningSession::sign_attestation(payload: impl Read) -> SigstoreResult<Bundle>
```

The API should:

1. Accept canonical in-toto Statement JSON as payload.
2. Set `payloadType` to `application/vnd.in-toto+json`.
3. Compute DSSE PAE exactly using byte lengths, not character counts.
4. Sign the PAE bytes, not the raw payload hash and not the manifest hash.
5. Submit the correct Rekor DSSE entry type, not `Hashedrekord`.
6. Emit a Sigstore Bundle v0.3 with `dsseEnvelope`.
7. Preserve the verification material needed for offline and online verification.
8. Include tests that prove the emitted bundle is DSSE-shaped and not MessageSignature-shaped.

This keeps Attestrum’s `attest_sign` as a thin wrapper and gives you a clean upstreamable patch.

### Step 3: Change Attestrum to consume the new fork API

In `crates/attestrum-attest/src/sign.rs`, replace:

```rust
session.sign(req.statement_payload)
```

with the new DSSE signing call.

The statement payload should stay as canonical in-toto JSON. The manifest file should remain the artifact being attested to through the in-toto Statement subject digest.

### Step 4: Keep verify focused on the manifest

For DSSE bundles, the existing verify shape is conceptually correct: pass the manifest file to sigstore-rs verification. sigstore-rs should verify the DSSE signature over PAE bytes and separately check that the in-toto Statement subject digest matches `SHA-256(manifest.parquet)`.

So do not change verify to feed Statement JSON unless choosing Option Z. Option Z should be rejected because it gives up the cosign interop promise.

### Step 5: Add explicit regression tests

Add tests for these cases:

1. Signing emits `mediaType` for Bundle v0.3.
2. Signing emits `dsseEnvelope`, not `messageSignature`.
3. DSSE `payloadType` equals `application/vnd.in-toto+json`.
4. DSSE payload decodes to the canonical in-toto Statement JSON.
5. Statement subject digest equals the manifest SHA-256.
6. Self-verify passes when given the manifest file.
7. Self-verify fails if the manifest changes.
8. Self-verify fails if the Statement subject digest is changed.
9. Upstream `cosign verify-blob-attestation --new-bundle-format` passes in CI.
10. The old MessageSignature path is not used by Attestrum attestation signing.

### Step 6: Update diagrams before code

Because the sign flow is changing, update the sign-flow Mermaid diagram before production code.

The diagram should show:

1. Manifest Parquet bytes.
2. SHA-256 manifest digest.
3. in-toto Statement subject digest.
4. DSSE envelope construction.
5. PAE byte construction.
6. PAE signature.
7. Rekor DSSE entry.
8. Bundle v0.3 assembly.
9. Cosign verification path.

Treat the diagram as the implementation map.

### Step 7: Do this before S5-D1 E5

This should land now, before API freeze and deterministic bundle golden work. A bundle shape change affects golden outputs, proof predicates, dataset cards, `attestrum prove`, and future verifier documentation. Do not freeze a known-wrong bundle contract.

## Option ranking

### 1. Modified Option Y: fork-first, upstream later

This is my preferred path.

It is only slightly slower than Option X, but cleaner. It keeps Sigstore signing behavior in sigstore-rs, gives Attestrum a small API surface, and creates a natural upstream PR later.

### 2. Option X: build DSSE directly inside Attestrum

This is acceptable only if the fork implementation becomes blocked.

It will probably work, but it puts low-level DSSE, Rekor, and bundle assembly logic into Attestrum. That is not ideal for maintenance, auditability, or future upstreaming.

### 3. Option W: shell out to cosign Go

This should be a fallback only.

It would likely fix interop quickly, but it cuts against the Rust-only CLI thesis and adds an external runtime dependency. It also weakens the story that Attestrum is a clean Rust-native Sigstore consumer.

### 4. Option Z: accept MessageSignature semantics

Reject this.

It is the smallest patch, but it breaks the main product promise. The goal is that third parties can verify with upstream cosign alone. Option Z permanently gives that up unless you later reissue or migrate bundles.

## Risk notes

### Rekor entry type is the highest-risk implementation detail

The DSSE envelope alone is not enough. The Rekor entry must match what cosign expects for a DSSE attestation bundle. If the signer emits DSSE-shaped bundle content but submits the wrong Rekor entry type, cosign can still reject it.

### Determinism needs careful language

The project’s byte-identical bundle goal is hard because real Sigstore signing includes timestamps, transparency log data, certificate material, and ECDSA signatures. Cross-target determinism can mean deterministic serialization of the same signing material, but live public-good signing will not produce identical bundle bytes across separate signing events.

I would explicitly separate two invariants:

1. Deterministic unsigned corpus artifacts, including manifest and statement payload.
2. Deterministic bundle serialization for fixed signing inputs and fixed verification material.

Do not accidentally promise that two independent live Sigstore signing events will produce identical bundle bytes.

### The existing workload-identity fork patch should stay

The `email` claim bug is real and orthogonal. Current upstream sigstore-rs main still shows `email: String` in `Claims`, so the fork remains necessary for GitHub Actions workload identity until that is upstreamed.

### The cosign module probably does not save you

The uploaded audit was right to check whether `src/cosign/intoto.rs` or `signature_layers.rs` hides a better signing API. From current upstream source, `cosign/intoto.rs` is just a re-export of bundle in-toto types, and `signature_layers.rs` is OCI signature-layer logic, not a public DSSE bundle signing API for this path.

## Final call

Proceed with modified Option Y immediately:

Implement DSSE signing in the existing sigstore-rs fork, expose it as a small signing-session API, wire Attestrum to it, update the sign-flow diagram first, then make cosign-interop the hard acceptance gate.

Do not pick Option Z. Do not freeze Sprint 5 APIs until this is resolved.
