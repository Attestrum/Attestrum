# Provenance Without Disclosure

### Cryptographic training-data provenance that keeps the corpus private — a design-and-explanation report from the Attestrum project

**Status:** research / explanation. Describes Attestrum's design and the privacy properties that follow from it. Claims are scoped to what the implementation and the underlying public specifications support; a dedicated "Limitations and threat model" section (§8) states what the system does *not* establish. This document references public standards (in-toto, Sigstore, RFC 6962, Croissant) by name; it does not name any organization as a user or customer, and it offers no legal advice.

---

## Abstract

Publishing the provenance of a training corpus appears to require a tradeoff: to let a third party *verify* what a dataset contains, you seemingly must *reveal* what it contains. For openly licensed corpora that tradeoff is acceptable. For a corpus that is itself a competitive asset, it is not — and a transparency obligation that can only be met by disclosure will be resisted rather than adopted. This report describes Attestrum, a deterministic command-line tool that compiles a corpus into a cryptographically verifiable provenance bundle, and explains the property that dissolves the apparent tradeoff: **Attestrum signs a fingerprint of the corpus, not the corpus.** The training data never leaves the machine that holds it. What can be published — at the publisher's discretion — is a small set of hashes, a signature, and a signing identity, any verifiable with standard open-source tooling and no Attestrum installation. We further describe how Merkle inclusion and non-inclusion proofs let a publisher answer a specific question — "is this work in the corpus?" — while disclosing only the answer for that one item. We are deliberate about limits: we state what a verifier *does* learn from each artifact, what the cryptography does and does not establish, and where trust assumptions remain.

---

## 1. The transparency–confidentiality tension

Two pressures push in opposite directions on anyone who trains a model on a large corpus. On one side, regulators, rightsholders, and downstream users increasingly want assurances about *what a model was trained on* — composition, licensing, the presence or absence of particular works. On the other, for some organizations the corpus is a primary competitive asset, and its exact contents are something they have strong commercial reason not to publish.

A naïve reading treats verifiable provenance and corpus confidentiality as mutually exclusive: either you publish the data (or a manifest detailed enough to reconstruct it) so others can check your claims, or you keep it private and ask to be taken at your word. Attestrum is built on the observation that this is a false choice for a large class of provenance claims. The claims that matter most — *this corpus has not been altered since it was sealed*; *it was sealed by this identity at this time*; *this specific work is, or is not, a member* — can be made checkable **without disclosing the corpus**, because they are claims about cryptographic commitments to the data rather than claims that require the data itself.

## 2. Design overview

Attestrum is a deterministic Rust command-line tool. Its pipeline, in stages:

- **build** — walk a corpus, fingerprint each document, and seal the result into a manifest committed to by a single root hash. Fingerprints and per-document metadata use published algorithms (for example BLAKE3 for content hashing); Attestrum does not invent new cryptographic or fingerprinting primitives.
- **sign** — wrap a statement about the sealed corpus in an in-toto attestation and sign it with Sigstore, producing a Sigstore bundle that standard tooling can verify.
- **verify** — check a bundle against an expected signing identity using stock tooling, with no Attestrum installation required.
- **prove** — emit a membership proof (inclusion or non-inclusion) for a queried work against a sealed corpus.

The artifacts are designed to be checked by widely used open-source software rather than by Attestrum itself: the Sigstore bundle verifies with `cosign`; the dataset descriptor is emitted as Croissant JSON-LD that validates against the public `mlcroissant` validator; the attestation uses in-toto predicate types. This "verifiable with stock tooling, no Attestrum required" property is a deliberate design goal, intended to avoid creating a dependence on Attestrum to trust an Attestrum-produced artifact.

The manifest is committed to using a binary Merkle tree following the construction described in RFC 6962 (Certificate Transparency), computed over BLAKE3 leaf and node hashes. The Merkle root is a single fixed-size hash that uniquely commits to the full ordered set of leaves: change any document, add one, or remove one, and the root changes.

## 3. What actually leaves the machine

The central privacy property follows from *what gets signed*. Attestrum does not sign the corpus; it signs a **digest**. The signed in-toto Statement's subject is a digest map (BLAKE3 and SHA-256 hashes), and its predicate carries summary fields such as the Merkle root and a row count — not document contents.

The sealed manifest itself stores, per document, a content hash, a size in bytes, a modality tag, an optional media type, provenance and licensing metadata (such as a source URL, registered domain, language, and SPDX license, where known), and a set of crawl/usage signal flags (for example robots and AI-usage directives). It does not store document bodies — there is no content, text, or raw-bytes column. A manifest is therefore a table of fingerprints and metadata, not a copy of the corpus. Some of that metadata — source URLs in particular — can itself be identifying; it lives only in the manifest, which a publisher of a private corpus can keep local and need not publish, and the membership proofs of §5 do not disclose it.

The consequence: across the build, sign, and verify stages, the raw bytes of the corpus are not transmitted off the machine. There is no stage whose job is to upload the data. The most that *can* be published, and only if the publisher chooses to, is a set of hashes, a signature, a certificate, and a timestamp.

This is worth stating precisely because it is easy to assume otherwise. A signed-provenance system *could* have been built to require uploading data to a server; Attestrum is not built that way. The data stays where it already is.

## 4. The transparency-log tradeoff

Sigstore's "keyless" signing flow obtains a short-lived certificate bound to an OpenID Connect identity and, by default, records the signing event in **Rekor**, a public append-only transparency log. Understanding what this does and does not expose is important, because it is the one place where signing — even though it never publishes the data — can publish *metadata*.

According to Sigstore's documentation, a Rekor entry stores the artifact **hash**, the signature, and the certificate (or public key) — not the artifact itself. The log is append-only and its entries are not designed to be deleted or altered; permanence is a property the log is built to provide, not a bug. In the keyless flow, the certificate embeds the OIDC identity (for example an email address or a CI workflow reference), and that identity is therefore recorded in the public log and is searchable there.

For an openly published dataset this is exactly the desired behavior: a public, tamper-evident, timestamped record that a named identity sealed a corpus with a given root hash. For a publisher who does not wish to expose even that metadata, several options exist in the Sigstore ecosystem, with tradeoffs:

- **Do not use the public log.** A publisher can keep everything local — Attestrum supports writing the full artifact set to a local directory (a "static" output target) with no network calls — and never contact a public Sigstore service at all.
- **Sign without uploading to the public log.** `cosign` supports disabling transparency-log upload; verification then requires explicitly accepting the absence of a log entry, which trades away the public-timestamp and discoverability benefits.
- **Key-based signing.** Signing with a managed key rather than the keyless flow records a public key rather than an email or workflow identity, which is pseudonymous, at the cost of key lifecycle management.
- **Self-hosted Sigstore.** Running a private Rekor (and optionally Fulcio) instance keeps the transparency log under the publisher's control.

In all of these, the data-stays-local property is unchanged; the choice governs only whether, and where, the *fingerprint and identity metadata* are recorded. The honest summary is therefore two-part: the corpus is never uploaded, and even the metadata footprint is something the publisher controls — up to and including "nothing public at all."

## 5. Proving composition without disclosure

Confidentiality of the corpus would be of limited value if it meant a publisher could never substantiate a specific claim about the corpus. Merkle commitments resolve this. Because the root hash commits to the full ordered set of leaves, a publisher can answer membership questions about individual items by revealing only a small *path* through the tree.

**Inclusion.** To show that a particular work is in the corpus, Attestrum produces an audit path: the set of sibling hashes along the route from that work's leaf to the root. A verifier holding the work's leaf hash, its position, the tree size, the claimed root, and the audit path can recompute the root and confirm membership. The verifier does not need the manifest and does not see any other document's content. (This is the same audit-path mechanism RFC 6962 uses for certificate transparency.)

**Non-inclusion.** To show that a work is *not* present, Attestrum relies on a defined leaf ordering: it discloses the two adjacent leaves that bracket where the queried item would fall, each with its own audit path, demonstrating that the queried key occupies the gap between them and therefore cannot be in the tree.

Both proofs are emitted as signed in-toto attestations under dedicated inclusion-proof and non-inclusion-proof predicate types, and are verifiable with the same stock tooling as the corpus attestation.

What a recipient of such a proof learns, stated plainly:

- For an **inclusion** proof: that the queried work is a member; the work's own hash and position; the sibling hashes on its path to the root; the root hash; and the corpus's size (leaf count). It does **not** learn the contents or identities of other documents.
- For a **non-inclusion** proof: that the queried work is absent; the queried key; the hashes and positions of the two bracketing leaves; and the corpus size. It learns two neighboring fingerprints, but not the corpus contents.

So the disclosure is bounded and item-scoped. A publisher can confirm or deny the presence of a specific work, and prove the corpus is unaltered, while revealing neither the rest of the corpus nor (with the log options of §4) anything in a public ledger.

## 6. Relevance to training-data transparency obligations

Emerging regulation — the EU AI Act's Article 53(1)(d) "training-content summary" obligation is the salient example — asks providers of general-purpose models to publish information about training content. Attestrum is positioned to produce cryptographic *evidence* that can back claims of the kind such obligations concern: a tamper-evident, timestamped, identity-bound commitment to a corpus, and item-level membership proofs against it. We are careful about the boundary here: Attestrum produces verifiable technical artifacts; it does not determine whether any particular disclosure satisfies a legal obligation, and this report offers no legal opinion. Whether and how these artifacts are used in a compliance context is a decision for the publisher and their counsel.

The design intent is narrow and worth stating to avoid overreach: Attestrum addresses the *training-data-provenance* slice — provable composition, integrity, and membership — and not the broader set of model-documentation, risk-assessment, or governance obligations that a full compliance program involves.

## 7. Vendor neutrality and determinism

Two engineering properties support the claims above.

**Vendor neutrality.** Every emitted artifact is designed to verify with standard public tooling and to use canonical public type identifiers (in-toto predicate types, the Sigstore bundle format, Croissant JSON-LD, and so on). The intent is that a recipient can verify an Attestrum artifact without trusting, or installing, Attestrum.

**Determinism.** Attestrum aims for byte-identical output across platforms: map iteration is ordered, timestamps derive from a single supplied source-date value rather than the wall clock, and serialization is canonical (sorted keys). This matters for provenance specifically — two independent runs over the same inputs should produce the same root hash and the same bundle bytes, so that a commitment is reproducible rather than dependent on the machine that produced it.

## 8. Limitations and threat model

This section is deliberately the most conservative in the report. A provenance tool that overstates what it proves is worse than none, because it invites misplaced trust.

- **Integrity of a *declared* corpus, not ground truth about training.** Attestrum proves that a corpus with a given root hash existed, was sealed by an identity, at a time, and that specific items are or are not members of *that sealed manifest*. It does not, by itself, prove that the manifest is a complete or truthful record of what was actually used to train any particular model. A binding from a corpus to a model is a separate, explicit attestation of a *claim*; the cryptography guarantees integrity, timestamp, identity, and verifiable membership against the corpus — not the truth of the training claim.
- **Membership is only as strong as the manifest it is checked against.** A membership proof recomputes a Merkle path against a manifest the verifier is given; it establishes a relationship to *that* manifest, not that the manifest is the one truly used.
- **What a proof recipient learns.** As detailed in §5, a verifier of a proof learns the corpus size, the queried item's membership and fingerprint, and (for non-inclusion) two neighboring fingerprints. These are small but real disclosures; "zero-knowledge" would be an inaccurate description and is not claimed.
- **Transparency-log metadata.** When the public keyless flow is used, identity and event metadata are recorded permanently and publicly (§4). This is avoidable, but only by an explicit choice; the default keyless behavior is not private.
- **Fingerprint matching.** Exact membership rests on cryptographic content hashing. Approximate or cross-format matching (for example perceptual or near-duplicate matching) relies on published heuristics with associated confidence levels, and is a weaker statement than an exact-hash match; some such matching is out of scope in the current version.
- **Trust assumptions.** Verification inherits the trust assumptions of the underlying ecosystem — the integrity of the Sigstore trust root and (when used) the transparency log, and the collision resistance of the hash functions employed.

## 9. Conclusion

The contribution Attestrum makes is not a new cryptographic primitive; it is an arrangement of existing, widely verifiable ones — content hashing, RFC 6962 Merkle commitments, in-toto attestations, and Sigstore signatures — into a workflow whose default is *the data does not move*. From that single design decision the privacy properties follow: a publisher can commit to a corpus, prove it unaltered, prove that specific works are present or absent, and choose exactly how much metadata, if any, becomes public — all without disclosing the corpus itself. For openly licensed datasets this adds tamper-evidence and portability. For datasets that must stay private, it offers something otherwise hard to obtain: a way to be checkable without being open.

---

## References

- in-toto Attestation Framework — predicate specifications and the in-toto Statement format.
- Sigstore — `cosign`, Fulcio, and the Rekor transparency log; see the Sigstore documentation for what the log stores and for signing options.
- RFC 6962, *Certificate Transparency* — the binary Merkle tree construction and audit-path mechanism.
- MLCommons Croissant — the dataset-description vocabulary and the `mlcroissant` validator.
- BLAKE3 — the content-hashing function used for leaves and nodes.

*Companion reports in this series: `docs/research/specification-first-agentic-engineering.md`, `docs/research/adversarial-review-high-stakes-decisions.md`, and `docs/research/cross-target-determinism.md`.*
