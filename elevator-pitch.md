# Elevator Pitch — Attestrum v2 (Path A)

> Trust layer for open AI training data. Deterministic compiler that turns a corpus into a cryptographically verifiable provenance bundle anyone can audit.

---

## Simple (one sentence, normie-friendly)

We're building the tool that proves what's actually inside an AI training dataset — so the people releasing the data can be trusted, and the people whose work was used can verify it.

---

## Medium (15 seconds, for a developer or VC)

Every AI dataset published on Hugging Face right now ships with a README and a prayer. There's no way to cryptographically prove what's in it, what's been removed, or whose work was opted out. We're building Attestrum — a deterministic compiler that turns any training corpus into a Sigstore-signed Merkle-rooted bundle plus a Croissant JSON-LD card and an EU Article 53 summary, in one command. The open community wants this, the EU regulation requires it, and Hugging Face has no native equivalent. Think Sigstore for AI training data.

---

## Long (60 seconds, for a partner meeting or design-partner call)

When Anthropic acquired Stainless for over $300 million in May 2026, the lesson was clear: frontier AI labs will pay nine figures for deterministic developer substrate. The same opportunity exists in AI training-data provenance — but the buyer isn't the frontier labs. The labs are actively litigating to keep their corpus details ambiguous, so a cryptographic proof of what they trained on is a liability for them, not an asset.

The buyer is the willing transparent middle. AI2 publishes Dolma openly. Pleias publishes Common Corpus, the largest open multilingual training set. EleutherAI co-published Common Pile, 8TB of public-domain text. Black Forest Labs is openly transparent about FLUX training data. Mozilla's Data Collective launched in 2025. These organizations want to ship cryptographically verifiable provenance and have no tool for it.

We're building Attestrum — a Rust CLI that takes a training corpus and produces, in one deterministic build, a BLAKE3 + RFC 6962 Merkle-rooted manifest, a Sigstore-signed in-toto attestation under three open predicate types we're submitting to the in-toto vetted catalog, a Croissant JSON-LD card for Hugging Face, an EU Article 53 training-data summary, and a static verification page anyone can use to audit the dataset without installing Attestrum. The same engine supports `attestrum prove` — give it a document fingerprint and a manifest, get back a signed inclusion or non-inclusion proof. That's the primitive rightsholders, auditors, and the EU AI Office have been asking for and don't currently have.

Open-source under Apache 2.0. Three-tier pricing: free OSS, $5–15K Pro for individual dataset publishers, $50–150K Enterprise for mid-tier GPAI providers. The acquirer story is Hugging Face wanting a trust layer for the Hub, Cloudflare extending the Human Native AI acquisition into compliance substrate, or Mozilla taking AI commons seriously. $50–200M outcome over three to five years. Healthy-business shape, not unicorn shape, but the buyer's incentive aligns with cryptographic certainty instead of fighting it.

---

## Full explanation (3 minutes, for a deep technical conversation)

### The market context

On May 18, 2026, Anthropic completed its acquisition of Stainless for over $300 million. Stainless did one technical thing extremely well — it ingested OpenAPI specifications and emitted production-ready type-safe SDKs across Python, TypeScript, Kotlin, Go, Java. OpenAI used it. Google used it. Cloudflare, Replicate, Anthropic itself. Multiple frontier labs had built on the same shared substrate, and Anthropic bought the substrate. The acquisition crystallized a pattern: frontier labs will pay nine figures for a single-purpose, deterministic, spec-driven artifact pipeline that becomes shared dependency.

The same structural opportunity exists in AI training-data provenance. The technical primitives are clear — content-addressed storage, Merkle trees, Sigstore attestation, deterministic builds. The artifact is clear — a cryptographically verifiable proof of what a training corpus contains and what's been removed. But the buyer is not who you'd think.

### Why not the frontier labs

The first version of this pitch aimed at frontier labs paying $250K–$1.5M for EU Article 53 compliance. A competitive audit killed that strategy. The labs are not stalling on Article 53 because the tool doesn't exist — they're stalling on purpose. A Sigstore-signed Merkle manifest of every training document is exactly the discovery evidence plaintiffs want in the Bartz v. Anthropic case (which settled for $1.5 billion), the NYT v. OpenAI case, and the music publisher cases. Frontier labs are litigating to keep their corpus ambiguous. They would not buy a tool whose output makes their training data auditable by plaintiffs and regulators.

This inverts the Stainless analogy. Stainless gave labs upside — better developer experience, no downside. Cryptographic corpus attestation gives labs downside — better evidence for the cases they're fighting. The acquisition logic collapses on inspection.

### Who actually wants this

The willing transparent middle. AI2 publishes Dolma openly under ODC-BY, with full per-document provenance. EleutherAI co-published Common Pile v0.1 with the University of Toronto, Hugging Face, MIT, CMU, and 14 other institutions — 8 TB of openly-licensed text designed for LLM pretraining. Pleias publishes Common Corpus, over 2 trillion permissibly-licensed multilingual tokens, framed explicitly as exceeding EU AI Act requirements. Black Forest Labs is transparent about FLUX training data composition. Mozilla launched the Data Collective at MozFest Barcelona in November 2025 as a non-profit dataset commons. Hugging Face hosts all of these and has been a primary Croissant adopter.

These organizations actively want to ship cryptographically verifiable provenance bundles and have no production tool for it. The Trinity College Dublin AI Accountability Lab audit of Article 53 summaries (January 2026) found that the only high-quality public training-data summaries were coming from small open-source labs — produced manually, at significant cost. They have demand. They have no supply.

### The product

Attestrum is a Rust CLI plus library that takes a training corpus and emits a complete provenance bundle in one deterministic build. Inputs: a `corpus.toml` describing data sources, the raw documents themselves (from local filesystem, S3, or Hugging Face), and opt-out signal sources — robots.txt with AI extensions, ai.txt from Spawning, W3C TDMRep, IETF AIPref, IPTC PLUS DMI, C2PA training-mining assertions, Really Simple Licensing from TollBit, Liccium TDM·AI, and Cloudflare Content Signals. Processing: parse all opt-out signals, content-hash every document with BLAKE3 and SHA-256, build an RFC 6962 binary Merkle tree, write to a content-addressed store, seal an Apache Parquet manifest sorted by document hash, sign the Merkle root with Sigstore Bundle v0.3 wrapping an in-toto Statement v1 under a new predicate type at `https://attestrum.com/attestation/training-corpus/v0.1`. Outputs: the Parquet manifest, the Merkle root, the Sigstore bundle, a Croissant JSON-LD card, a CycloneDX 1.7 ML-BOM, a Hugging Face dataset card README with YAML frontmatter, the EU Article 53 training-data summary template auto-populated, and a self-contained static `verify.html` page anyone can use to audit the dataset without installing Attestrum.

The headline command for Path A isn't `attestrum build` — it's `attestrum prove`. Give it a document fingerprint (BLAKE3 hash, ISCC under ISO 24138:2024, perceptual hash for images and audio, or normalized MinHash and SimHash for text) and a manifest reference (local Parquet, Hugging Face Hub URL, or registry URL), and `attestrum prove` returns either a signed inclusion proof with a Merkle audit path, or a signed non-inclusion proof using the sorted-Merkle adjacent-neighbors technique. Both proofs are emitted as separate Sigstore bundles under two new predicate types — `inclusion-proof/v0.1` and `non-inclusion-proof/v0.1` — that reference the corpus attestation by digest. This is the primitive rightsholders, auditors, and litigators have been asking for and that nobody currently provides.

### The competitive landscape

Nothing in the world is doing the exact Attestrum stack — Rust CLI plus raw corpus input plus Sigstore plus RFC 6962 Merkle plus Croissant plus EU Article 53 plus opt-out signal ingestion plus inclusion and non-inclusion proofs plus open source. But three adjacencies are close enough that the pitch has to acknowledge them, and one of them could plausibly expand into the same lane within twelve to eighteen months.

The closest is the **OWASP AIBOM Generator**, originally an Aetheris AI project that relocated to the OWASP GenAI Security Project in December 2025. It runs as a Hugging Face Space, is listed in the CycloneDX Tool Center, holds weekly community meetings, and has an in-progress AIBOM Generation Handbook. Given a Hugging Face model ID, it scrapes the model card and repository and emits a CycloneDX 1.6 JSON AIBOM with a completeness score. What it does not do is the cryptographic primitive layer — no Sigstore signing, no Merkle tree, no in-toto attestation, no inclusion proofs, no opt-out signal ingestion, no EU Article 53 summary, no Croissant card, no training-corpus input. Same audience (Hugging Face, open-source AI compliance), same regulatory tailwind, OWASP brand power, and zero of the cryptographic surface Attestrum provides. If they bolt on Sigstore signing in 2026, the gap narrows fast. The right move is to engage the maintainers early, position Attestrum as the layer beneath their AIBOM, and feed Attestrum outputs into their completeness scoring.

The second is the **Sigstore Model Transparency project** (`sigstore/model-transparency`) and the broader **OpenSSF Model Signing (OMS)** specification it produced. Google, NVIDIA, Red Hat, and HiddenLayer are all involved. OMS is integrated into Kaggle and NVIDIA NGC. The project signs ML *models* — weights files — using Sigstore, with optional Rekor transparency log entries. The corpus side of the supply chain is entirely outside its scope. There is no Merkle commitment over training data, no document-level inclusion proof, no opt-out signal handling. The right framing here is the parallel ecosystem: OMS is what Sigstore did for model weights, Attestrum is what Sigstore should be for training corpora. The model-signing world is one inch over, and the alignment story writes itself if the predicate types Attestrum proposes get accepted into the same in-toto vetted catalog OMS already lives in.

The third is **DataTrails**, formerly RKVST, founded 2018 and acquired by ONID in August 2025. Provenance-as-a-Service: SaaS API plus distributed ledger backend, compliant with the IETF SCITT framework, marketed for training-dataset lineage among other use cases, partnered with Digimarc on watermarking. Different shape from Attestrum on three dimensions — proprietary SaaS instead of open-source CLI, blockchain instead of Sigstore, generic enterprise provenance instead of training-corpus-specific compilation. They have patents and a sales motion, but mid-acquisition attention typically slows competitive pivots by twelve to eighteen months. The risk is a post-integration repositioning toward "Attestrum but proprietary"; the structural mitigation is the open-source posture itself, which is incompatible with their pricing model.

Secondary mentions worth tracking but not fearing. The **draft-sharif-ai-model-lifecycle-attestation-00** IETF Internet-Draft from March 2026 proposes a similar primitive — ECDSA P-256, SHA-256, Merkle trees for corpus attestation — but it is an individual draft by a single author, no working group adoption, no formal IETF standing. The right move is to cite it in Attestrum's alignment story, not avoid it. The **ogulcanaydogan/LLM-Supply-Chain-Attestation** GitHub project from February 2026 is a solo-developer effort with broader scope (prompts, training data, evaluations, routing, SLOs) using Sigstore keyless plus OCI plus OPA — no visible traction, no corpus focus, worth a watch but not a competitor. Academic work — **ZKPROV**, **VFT (Verifiable Fine-Tuning)**, **Proof-of-Training-Data**, and the **ALOHA** AIBoM tool — uses fundamentally different technical approaches (zero-knowledge proofs over training computation rather than Merkle attestation over corpus content), and none are products. They become useful citations for the technical paper later.

Several things that pattern-match but are clearly not in the same lane. **C2PA**, **Truepic**, **TrueScreen**, and **Numbers Protocol** are content credentials for media outputs at point of generation — Article 50 territory, not Article 53. Adjacent only at the level of "cryptographic provenance." **Croissant** is a metadata format, not a signing or attestation tool — substrate, not competitor. **Scale AI**, **Appen**, **Cogito**, and **iMerit** are training-data vendors, a different industry entirely. **Spawning**, **Dark Visitors**, **Cloudflare Pay Per Crawl**, **TollBit RSL**, and **Liccium** are opt-out signal *publishers* — they produce the inputs Attestrum consumes, which makes them complementary rather than competitive. Various blockchain-for-AI-provenance proposals exist as conceptual writeups but have no production presence in the willing-transparent-middle market.

The honest, refined version of the "no funded competitor" claim: no funded competitor produces a deterministic, Sigstore-signed, Merkle-rooted bundle for training corpora specifically. Model-signing covers the adjacent layer above, AIBOM generation covers the adjacent layer beside, and DataTrails covers the generic provenance SaaS market with a different technical substrate. Attestrum is the missing corpus-layer compiler beneath all three, and the only one of those positions that is both unclaimed and structurally aligned with the willing transparent middle's open-source posture.

### Why this survives LLM commodification

The output is a cryptographic artifact. An LLM cannot fabricate a Merkle root over twelve terabytes of training data. The signed manifest is verifiable by anyone with the source data. The Sigstore bundle and the in-toto Statement must conform exactly to their public schemas — wrong by one byte and `cosign verify-blob-attestation --new-bundle-format` rejects it. The proof types must mathematically check out against the published Merkle root. This is the same structural moat that protected Stainless: byte-deterministic, type-aware, spec-driven artifacts that have to be exactly right.

### Why a solo founder ships this in 90 days

The technical surface is well-bounded. The hard skills are build-system literacy — Merkle trees, content-addressed storage, reproducible builds — and regulatory fluency around Article 53, Attestrum XI, the CDSM Article 4(3) opt-out regime, and the Croissant ML metadata spec. There is no machine-learning research required. Claude Code handles implementation velocity at this surface area. Sprints 1 through 5 ship the compiler, the Merkle tree, the Sigstore integration, the fingerprinting, and the proof primitive. Sprint 6 ships Hugging Face Hub publishing and the end-to-end public-verification demo. Every sprint begins with Mermaid diagrams checked into `docs/diagrams/` before any code is written — a hard CI gate enforces this, because diagram-versus-code drift is the most expensive bug class in cryptographic systems.

### The business model

Open-source CLI under Apache 2.0 to seed adoption. Three pricing tiers above it. Free for individuals and small open-source projects. Pro at $5–15K per year for individual dataset publishers and small labs who want managed sealing, attestation hosting, and registry federation. Enterprise at $50–150K per year for mid-tier GPAI providers, EU-resident fine-tuners, and dataset publishers who need audited build infrastructure, witness operation, and compliance indemnity. Path to $10–30 million ARR within 30 months. Skip the $250K–$1.5M frontier-lab tier — that buyer isn't real.

### The acquisition logic

The natural acquirers are Hugging Face wanting a native trust layer for the Hub, Cloudflare extending the Human Native AI acquisition from January 2025 into compliance substrate for their Pay Per Crawl ecosystem, or Mozilla taking AI commons infrastructure seriously through the Data Collective. The strategic pitch is the same Sigstore-to-Chainguard pattern: become the canonical open-source name in training-data attestation, get the predicate types accepted into the in-toto vetted catalog and the SPDX 3.0 AI profile, and become the substrate every acquirer benefits from owning. $50–200 million outcome over three to five years. Not Stainless-shape. Healthy-business shape, with optional asymmetric upside if AI copyright litigation builds the larger eDiscovery market we'd address as a v2 product line into Thomson Reuters, RELX, or Relativity.

### The first design partners

AI2 / Allen Institute on Dolma v1.7. Pleias on Common Corpus. EleutherAI on Common Pile v0.1. Black Forest Labs on FLUX training data (image modality validation). Mozilla Data Collective on best-practices adoption. Hugging Face's Datasets team as the distribution partner, not a customer per se but the wedge that gets us a placement on `huggingface.co/blog` and integration with the Hub's existing dataset surface. None of these conversations require us to be a lawyer. None require litigation-tech sales credibility. All five care actively about the artifact we produce.

### Bottom line

The market exists, the regulatory tailwind is real, the buyer's incentives align with cryptographic certainty rather than fighting it, the technical surface is well-bounded, the standards substrate is being defined in public, and no funded competitor produces the deterministic open-source CLI for training-corpus attestation — the closest adjacencies sit one layer up (Sigstore Model Transparency), one layer beside (OWASP AIBOM Generator), or one shape over (DataTrails). Article 53 enforcement begins August 2, 2026. The willing transparent middle wants the tool now. Twelve weeks to ship. Build it.
