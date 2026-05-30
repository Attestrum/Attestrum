# Migration / catalog packet: `model-binding/v0.1` (new predicate)

**Type**: NEW PROTECTED predicate URI — a separate v0.1 generation, not a change to the v0.3 corpus/proof family.
**Date**: 2026-05-29.
**Introduced by**: the corpus-to-model binding promotion (`MODEL_BINDING_PREDICATE_TYPE`, `crates/attestrum-attest/src/model_binding.rs`).
**URI**: `https://attestrum.com/attestation/model-binding/v0.1`
**Schema**: `docs/schemas/model-binding-v0.1.schema.json` (schemars-derived from `ModelBindingPredicate`).
**Submission target**: in-toto vetted catalog at <https://github.com/in-toto/attestation/tree/main/spec/predicates>. **Not yet submitted** — the catalog PR opens once `attestrum.com` serves the schema URLs; this packet is the prepared submission record.
**Approval**: protected-system change approved 2026-05-29 (decision protocol `model-binding-v0-1-promotion`, founder Austin Munday). Commit `43ae8b4` introduced the URI under the §4 ceremony.

---

## What this is

A signed Attestrum corpus attestation proves "corpus C existed with Merkle root R" — **not** "C is what trained model M." The `model-binding/v0.1` predicate closes that gap. It is an in-toto v1 Statement, SLSA-shaped:

- **`subject`** = the model (its weights-manifest digest; the model-card / release URI as the subject `name`).
- **`predicate.corpora`** = the materials: one or more training-corpus attestations the trainer claims produced the model, each referenced by its canonical attestation digest + Merkle root + manifest digest + a free-text `role`.
- **`predicate.model`** / **`predicate.training`** = model identity (+ optional OpenSSF/Sigstore `signingBundleRef`, recorded-not-verified at v0.1) and contemporaneous, deterministic training metadata.

## Honest ceiling (foundational — restate to every consumer)

This is an **attestation, not a proof-of-training**. The cryptography guarantees integrity, timestamp, identity, and verifiable membership against C — **not** the truth of the training claim, and (per decision D6) **not** an independent attestation of a work's membership: the signed chain walk (`attestrum walk-chain`) *recomputes* the inclusion/non-inclusion proof live from the verifier-supplied manifest rather than verifying an independently-signed one. Membership is therefore only as strong as the manifest fed to the prover; a dishonest trainer can attest a sanitized corpus. Mitigations are contemporaneousness, Sigstore identity-binding, the transparency-log timestamp, and that lying in a signed, logged attestation is perjury-adjacent. This is the same ceiling as the EU Article 53 training-content summary.

## Why a new URI and not a v0.4 of an existing predicate

The binding is a genuinely new artifact (a new subject/materials shape), not a change to `training-corpus/v0.3` or the proof predicates. Per CLAUDE.md §4 a new predicate gets its own URI at its own version line. `attestrum_attest::ALL_PREDICATE_TYPES` is **deliberately left frozen at the `[3]` v0.3 corpus/proof snapshot** (decision D2-A) — `MODEL_BINDING_PREDICATE_TYPE` is a standalone const, not a member, because the two are independent generations and a "complete list of every predicate" stops being meaningful once versions diverge.

## Bundle impact

**None.** No `model-binding/v0.1` bundle has been published. The first will be emitted by `attestrum bind`. There is nothing to migrate.

## Cross-link: the proof-predicate determinism amendment

The eventual catalog submission bundles two records: this new predicate **and** the `attestationDigest` determinism correction to the existing proof predicates documented in [`v0.3-attestation-digest-determinism.md`](v0.3-attestation-digest-determinism.md) (commit `3cbeee7`). Both must land in the proof-predicate / binding catalog packet so the submitted spec satisfies Attestrum's own §7 determinism invariant. The binding's `corpora[].attestationDigest` is computed by the **same** canonical primitive (`attestrum_attest::attestation_digest_of_bundle` / `attestation_digest_of_statement`), so the determinism correction and the binding share one digest definition.

## Verification

Every `model-binding/v0.1` artifact verifies with standard public tooling (CLAUDE.md §12): the in-toto Statement validates against the published schema, and the Sigstore Bundle v0.3 verifies with `cosign v3+ verify-blob-attestation --new-bundle-format` and no Attestrum install. The string "attestrum" appears only in the predicate URI prefix and the informational `builderVersion` field.
