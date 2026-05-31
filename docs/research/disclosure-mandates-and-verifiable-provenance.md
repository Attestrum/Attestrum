# The Disclosure Floor and the Defensible Position

### How verifiable training-data provenance backs the new transparency mandates — a positioning-and-explanation report from the Attestrum project

**Status:** research / explanation. Summarizes what two training-data transparency regimes — California AB 2013 and Article 53(1)(d) of the EU AI Act — require *of their published outputs*, citing primary sources, and explains where a cryptographically verifiable corpus record is, and is not, relevant to them. This document restates what the regulations and the AI Office template say; it does **not** offer legal advice, does not opine on whether any artifact satisfies any obligation, and names no organization as a user. Its companion `provenance-without-disclosure.md` covers the privacy properties and the full threat model referenced here.

---

## Abstract

Two new regimes require disclosure of what a model was trained on: California's AB 2013 (in effect 1 January 2026) and Article 53(1)(d) of the EU AI Act (applicable to new general-purpose models from 2 August 2025, with enforcement powers from 2 August 2026). A careful reading of both shows that the *required output* in each case is a **structured narrative summary** — prose describing data sources, scale, processing, and copyright posture, with a handful of enumerated lists — and not a manifest, a hash, or a proof. This report makes one distinction precise: the mandated summary is a **floor** — an assertion a reader is asked to take on trust — and a verifiable provenance record is a separate **backing layer** that makes the factual spine of that assertion independently checkable. We map each regime's required fields onto what a deterministic, signed, Merkle-committed corpus record can and cannot substantiate, and we are explicit that the legal-judgment portions of these disclosures (copyright status, licensing, personal-data determinations) are human determinations that no provenance tool produces. The contribution is conceptual, not a claim that either law requires cryptography: it does not. The claim is narrower and, we think, more useful — that a published summary and a verifiable record answer two different questions, and the second is the one a challenger can independently test.

---

## 1. Two mandates, one shared shape

**California AB 2013** (Generative Artificial Intelligence: Training Data Transparency Act) requires a developer of a generative AI system made available to Californians to post, publicly, a **high-level summary of the datasets** used to develop the system. The statute enumerates twelve categories the summary must address — among them the sources or owners of the datasets, the number and types of data points, whether the data includes copyrighted/trademarked/patented or public-domain material, whether it was purchased or licensed, whether it contains personal information, any cleaning or processing applied, the collection time period, the dates of first use, and whether synthetic data was used. The obligation applies on or before 1 January 2026 and again before each subsequent release or substantial modification, reaching systems released since 1 January 2022. The statute does not define "high-level," and explicitly does not call for item-by-item disclosure.

**EU AI Act Article 53(1)(d)** requires a provider of a general-purpose AI (GPAI) model to make publicly available a **"sufficiently detailed summary of the content used for training"**, drawn up according to a template provided by the AI Office. That template, published on 24 July 2025, organises the summary into three blocks: general information (provider and model identity, modalities present and their approximate scale, content types); a list of data sources (primary public and licensed datasets; web-scraped data, described narratively and including a list of the top 10% of scraped internet domain names ranked by the size of content scraped — for SMEs, the top 5% or 1,000 domains, whichever is lower; and other sources such as user or synthetic data); and data-processing aspects (processing applied before training, measures for copyright and text-and-data-mining opt-out compliance, and removal of illegal content). The obligation applies to new models from 2 August 2025; the Commission's enforcement powers, including fines, apply from 2 August 2026; models already on the market before 2 August 2025 have until 2 August 2027 to comply.

The two regimes differ in scope, teeth, and jurisdiction, but their *required output has the same shape*: **a structured narrative**. Both ask the developer to **describe**. Neither asks the developer to **prove**, and neither specifies any format that a third party could mechanically verify. This is not a criticism of either instrument — a narrative summary is a reasonable transparency floor. It is simply the fact that frames everything below.

## 2. The gap a narrative leaves open

A published summary is an **assertion**. It states that the corpus drew on certain sources, at a certain scale, processed in certain ways. A reader — a downstream developer, an auditor, a rightsholder, a regulator — is asked to take the assertion on trust, because the summary carries nothing that lets the reader check it independently. Three questions sit outside what either mandate's output can answer:

1. **Is the published summary accurate** — does it faithfully describe the corpus that was actually sealed?
2. **Has the corpus been altered** since the summary was written?
3. **Is a specific work present or absent** — the question a rightsholder most often actually asks?

None of these is answerable from prose. They are not defects the summary was meant to address; they are simply a different layer of assurance. Answering them requires a commitment to the data that a third party can test — which is what a verifiable provenance record provides, and what the rest of this report describes.

## 3. Verifiable provenance as a backing layer

A provenance record of the kind Attestrum produces does not replace the mandated summary; it sits underneath it and makes its factual spine checkable. The mechanism is described in full in the companion report and in the project's end-to-end documentation; in brief:

- A corpus is compiled into a **manifest** committed to by a single **Merkle root** (the RFC 6962 binary-tree construction over BLAKE3). Under standard hash collision-resistance assumptions the root fixes the full ordered set of documents: alter, add, or remove one and the root changes. This is the backing for question 2 — *alteration*.
- A statement about the sealed corpus is wrapped in an **in-toto attestation** and **Sigstore-signed**, binding the root to a signing identity and a timestamp; the resulting Sigstore bundle is designed to verify with stock `cosign`, no Attestrum installation required.
- **Inclusion** and **non-inclusion proofs** answer question 3 for a *single* queried work, disclosing only a short Merkle path rather than the corpus. A publisher can confirm or deny one item's membership without revealing the rest.
- The descriptive artifacts the mandates concern — modality counts, data-source categories, a top-domain list for scraped data (the template's top 10% by content size), the processing applied — are **derived deterministically from the sealed manifest**, so the same inputs always produce the same description. That reproducibility is what lets a third party *with access to the manifest* re-derive a published number or domain list and check it against the record it claims to summarise. In the confidential-corpus mode of the companion report the manifest stays private, so this check is available to a holder of the manifest — an auditor under NDA, say — rather than to the general public, for whom the item-scoped membership proofs above are the disclosure-bounded alternative. This is the backing for question 1 — *accuracy* — to the extent the field is a factual one, and only against the manifest as sealed.

The key word is *factual*. Where a mandated field is a measurable property of the corpus — how many documents, which modalities, which scraped domains, what processing — a verifiable record can substantiate it. Where a field is a **legal determination** — whether content is copyrighted, whether a licence was valid, whether data is "personal information," whether a text-and-data-mining opt-out was honoured — no provenance tool decides it. Those remain human judgments, and Attestrum neither makes them nor claims to.

## 4. Field-by-field: what a record can and cannot back

The table maps the two mandates' fields to what a verifiable corpus record substantiates. *Backs* = the record makes the field independently checkable; *Records* = the record can carry the value if supplied at ingest, but does not determine it; *Out of scope* = a legal or human determination the tool does not make.

| Disclosed field | AB 2013 | EU template | Verifiable record |
|---|---|---|---|
| Data sources / owners | ✓ | ✓ | **Backs** (per-object provenance) |
| Number of data points / size ranges | ✓ | ✓ | **Backs** (exact, deterministic count) |
| Types / modalities of data | ✓ | ✓ | **Backs** (classified at build) |
| Top-domain list (top 10% by scraped content) | — | ✓ | **Backs** (registered-domain field, derived) |
| Cleaning / processing applied | ✓ | ✓ | **Backs** for the normalisation Attestrum applies; **Records** upstream cleaning (as asserted) |
| Collection period / first-use dates | ✓ | — | **Records** (as supplied) |
| Copyright / IP status | ✓ | ✓ | **Out of scope** (legal determination) |
| Licensed vs purchased | ✓ | ✓ | **Records** (e.g. SPDX field, if supplied) |
| Personal / aggregate consumer information | ✓ | — | **Out of scope** (legal determination) |
| TDM opt-out compliance | — | ✓ | **Records** (crawl/AI-usage signal flags) |
| Synthetic data used | ✓ | ✓ | **Records** (if tagged at ingest) |

The pattern is consistent: a verifiable record backs the *quantitative and structural* half of both disclosures — the part that is a fact about the corpus — and merely carries, or stays out of, the *legal-judgment* half. A summary built on such a record is not "more compliant"; it is **more defensible**, because its checkable claims can actually be checked.

## 5. What this does and does not establish

This is the section we are most conservative about, because a provenance tool that overstates what it proves invites misplaced trust. The full threat model is in `provenance-without-disclosure.md` §8; the essentials, restated so this report stands alone:

- **Neither mandate requires cryptographic provenance.** Both are satisfied, as a matter of their stated text, by a narrative summary. Nothing here should be read as a claim that a verifiable record is legally required. It is not. It is an assurance layer a publisher may add.
- **Integrity of a *declared* corpus, not ground truth about training.** A verifiable record proves a corpus with a given root existed, was sealed by an identity, at a time, and that specific items are or are not members of *that sealed manifest*. It does not prove the manifest is a complete or truthful account of what actually trained a model. A corpus-to-model binding is a separate, explicit attestation of a *claim*; the cryptography guarantees integrity, timestamp, identity, and membership against the record — not the truth of the training claim. A publisher who seals a sanitised corpus is not caught by cryptography alone.
- **Membership is only as strong as the manifest checked against.** A proof establishes a relationship to *a* manifest, not that the manifest is the one truly used.
- **The legal half stays human.** Copyright, licensing validity, personal-data classification, and opt-out compliance are determinations for the publisher and their counsel. This report takes no position on any of them.

Stated plainly: the mandated summary is the **floor**, and it is a floor reachable with prose. A verifiable record is a **defensible position** above that floor — it answers "is this accurate, unaltered, and does it contain X?" — and it answers those questions only for the factual claims, only against the record sealed, and only when the publisher chose to seal honestly. That bounded honesty is the point. An assurance layer is useful in proportion to how precisely it states its own limits.

## 6. Conclusion

The two transparency mandates examined here ask developers to *describe* their training data, and a description is an assertion offered on trust. That is an appropriate floor and a real step forward for transparency. It also leaves three questions — accuracy, integrity, and specific membership — that no narrative can answer, because answering them requires a commitment to the data that a third party can test. Verifiable training-data provenance is exactly such a commitment, assembled from existing, independently verifiable primitives. It does not make a publisher *more compliant* with laws that do not require it; it makes the factual spine of a compliant summary *checkable*, and therefore defensible, while leaving every legal judgment where it belongs — with the publisher and their counsel. Disclosure is the floor. A record someone else can verify is the position you can defend.

---

## References

- California AB 2013, *Generative Artificial Intelligence: Training Data Transparency* — California Legislature (bill text and enrolled summary).
- Regulation (EU) 2024/1689 (AI Act), **Article 53** — obligations for providers of general-purpose AI models.
- European Commission / AI Office — *Explanatory Notice and Template for the Public Summary of Training Content for general-purpose AI models* (published 24 July 2025).
- in-toto Attestation Framework; Sigstore (`cosign`, Fulcio, Rekor); RFC 6962, *Certificate Transparency*; MLCommons Croissant; BLAKE3 — the underlying public standards (see `provenance-without-disclosure.md` for usage detail).

*Companion reports in this series: `docs/research/provenance-without-disclosure.md` (privacy properties and threat model), `docs/research/how-attestrum-works-end-to-end.md`, `docs/research/cross-target-determinism.md`.*
