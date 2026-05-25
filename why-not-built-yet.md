# Why Hasn't Anyone Built Attestrum Yet?

> A strategic audit of the structural reasons this gap exists, why it survived this long, and why the conditions producing the gap are now relaxing simultaneously.

---

## The question worth interrogating

When you find a gap that looks obvious — a primitive the market clearly needs, that the standards substrate supports, that the regulatory environment is actively asking for, that has no funded competitor — the honest first reaction should be suspicion, not excitement. Gaps that look obvious almost always have reasons. Sometimes those reasons are still in force, in which case the gap isn't a gap, it's a moat protecting somebody else's better idea. Sometimes the reasons used to be in force and just stopped being in force, which is what creates the narrow windows that matter for founders.

The Attestrum gap is the second kind. There are at least eight structural reasons nobody has built this. All of them used to be binding. Most of them are relaxing right now, simultaneously, for unrelated reasons. The composition of the relaxations is what creates the window. The fact that those relaxations are visible and dated is what makes the window narrow.

This document walks each reason carefully, layers in the technical observation about the specific unclaimed primitive, and closes with the timing analysis.

---

## Reason 1: The obvious buyer is structurally opposed

The first version of the Attestrum pitch aimed at the most natural-seeming customer — frontier AI labs paying $250K–$1.5M for EU Article 53 compliance tooling. A few hours of competitive analysis killed that strategy. The labs are not stalling on Article 53 because the tool doesn't exist. They're stalling on purpose.

Cryptographic corpus attestation produces, by design, the exact evidence that plaintiffs are demanding in active litigation. The Bartz v. Anthropic case settled for $1.5 billion in 2025 — without that level of discovery. The NYT v. OpenAI case is ongoing. Music publisher cases against multiple labs. The Getty v. Stability AI case in the UK. Authors Guild cases. Each one of these depends on the defendant being unable to produce a precise, complete, byte-deterministic accounting of what was in the training corpus. A Sigstore-signed Merkle manifest of every training document is the plaintiff's dream subpoena response. Frontier labs are paying their lawyers a lot of money to maintain ambiguity. They would not buy a tool whose output makes their training data auditable by opposing counsel.

This inverts the analogy to deterministic developer-substrate plays. Stainless gave frontier labs upside (better developer experience for their API users, no downside). Cryptographic corpus attestation gives frontier labs downside (better evidence for the cases they're fighting, no upside they can't get more cheaply by writing a README themselves). The acquisition logic that worked for Stainless collapses on inspection for corpus attestation.

The second-order effect is what drains the funded-startup pipeline. Most founders who chase AI-infrastructure money pattern-match to "sell to OpenAI / Anthropic / Google at $10M+ ACVs." They look at the corpus-attestation gap, see no buyer in their target list, and walk away. VCs do the same pattern-matching when they pass on the deal. Five years of this dynamic has produced a vacuum where the obvious adjacent technical work happens (Sigstore Model Transparency, Atlas, AIBOM) but the specific corpus-compile primitive doesn't get funded because nobody is willing to abandon the frontier-lab buyer and reach for the smaller transparent middle.

The deeper structural point: in most markets, the largest and best-funded buyer pulls toolmakers toward itself. The build-vs-buy economics favor expensive tools where the buyer is willing to pay. In this market the largest buyer has actively negative pull. That's an unusual market shape, and it's the kind of shape that produces persistent vacuums. The vacuum stays because the gravity is reversed, not because the gap is invisible.

---

## Reason 2: The actual buyer is a brand-new audience

The market that does want this — what the pitch calls the "willing transparent middle" — wasn't a coherent, addressable audience until very recently. The component organizations existed individually for years, but they didn't form a recognizable category that a founder could point at and pitch.

AI2's Dolma dataset was first released in February 2024 and expanded through 2025. EleutherAI co-published Common Pile v0.1 in 2025 as 8TB of public-domain training text. Pleias's Common Corpus, currently the largest open multilingual training set, came together over 2024 and expanded in 2025. Black Forest Labs adopted its transparent posture around FLUX training data after its 2024 founding. The Mozilla Data Collective launched in 2025 as an explicit initiative to build AI training-data commons. Each of these organizations existed individually, but the category — "transparent AI dataset publishers who would adopt cryptographic provenance tooling" — only became legible as an audience in roughly the last twelve months.

Before that window, a founder thinking about this space would have seen AI2 (a research institute), Mozilla (a foundation), EleutherAI (a collective), and the various smaller publishers as disconnected entities with different funding models and no shared purchase pattern. There was no obvious meeting room where you'd pitch them all. There still isn't, really, but the category has cohered enough that the publication patterns (Dolma releases, Common Pile drops, Common Corpus updates) now form a visible rhythm. A toolmaker can now point at the rhythm.

The composition matters because these organizations have small individual budgets but high coordination. They watch each other. They reference each other's work in their dataset cards. When one of them adopts a tool, the others investigate. The interoperability incentive is strong because the goal is shared — building AI commons that downstream researchers can actually use. So even though no single one of them will pay enterprise rates, the cumulative effect of standard adoption across the group has historically been substantial. See the spread of model cards, datasheets for datasets, and Croissant itself for the pattern: each of those went from "interesting proposal" to "expected best practice" within eighteen to thirty-six months of the first publication adopting them.

The other half of the audience that hasn't been discussed enough: mid-tier GPAI providers who are EU-resident, who can't afford frontier-lab legal strategies, and who genuinely need a clean compliance story because they don't have the legal budget to argue ambiguity. Mistral. Aleph Alpha. Smaller European AI shops. They want a defensible answer to "what's in your training data?" because they don't have the resources to defend the indefensible. Those companies didn't exist in their current form three years ago. They constitute a second wing of the willing-transparent-middle that's growing for regulatory rather than philosophical reasons.

---

## Reason 3: The regulatory deadline hasn't bitten yet

Article 53 enforcement begins August 2, 2026 — about ten weeks from now. The template that providers have to fill out was only adopted by the European Commission on July 24, 2025. Before July 2025, the law existed but the operational specifics didn't. Between July 2025 and August 2026 we've been in the regulatory equivalent of a phony war: the rules exist, the fines exist on paper, but no enforcement action has landed, no precedent has been set, and no in-house compliance team has felt the political pressure that turns "we should look at this someday" into "we're buying this next quarter."

This is the standard shape of regulatory-driven tool markets. GDPR took effect in May 2018. The cottage industry of consent-management tools didn't really materialize until 2019–2020, when enforcement actions started landing and B2B buyers got nervous. Sarbanes-Oxley passed in 2002; the compliance-tech market it generated didn't peak until 2005–2008. HIPAA: similar pattern. PCI-DSS: similar pattern. CCPA: similar pattern. There's a roughly six- to eighteen-month lag between "the regulation has teeth" and "buyers are buying urgently," because compliance procurement is panic-driven and the panic doesn't kick in until an enforcement action lands on a recognizable peer.

What this means for Attestrum: most regulatory-driven tools that get built in this market are built during the panic-procurement window (months 6–18 post-enforcement). Attestrum is being built in the ten weeks before the window even opens. That's the unusual move. It's earlier than the standard founder timing because the window for first-mover advantage on the in-toto vetted catalog predicate types is shorter than the panic window, and you have to be in the catalog when the panic starts in order to ride it.

There's a deeper point about how regulatory deadlines interact with standards races. Once enforcement begins, the first tool with credible adoption tends to become the de facto answer to "how do we comply?" within six to twelve months. Companies hate evaluating tools during a panic. They want to buy what their peers bought, what the regulator has seen and not objected to, what the auditor recognizes. The standards-race winner is usually the tool that was already deployed and credible when the deadline hit. Showing up six months after enforcement is too late — the cohort that bought first is already the answer.

So the Attestrum timing isn't "ship a tool to capture demand once demand exists." It's "ship a tool, get it adopted by enough trusted reference customers in the open transparent middle, get the predicate types into the in-toto vetted catalog, get integrated with Hugging Face's publish flow, and then be sitting there visible and credible when the first enforcement action lands on a regulated entity that has to scramble." The ten weeks before August 2 are the build window. The twelve months after August 2 are the adoption window. Both have to happen, in order, with no gap.

---

## Reason 4: The standards substrate only just composed

Each of the cryptographic primitives Attestrum uses has been individually mature for a while. BLAKE3 stabilized in 2020–2021. RFC 6962 Certificate Transparency Merkle trees have been in production at Google for years. Sigstore reached v1.0 in 2022. The in-toto Attestation Framework released v1 in 2023. None of these are new.

What's new is the composition. The Sigstore Model Transparency project reached v1.0 in mid-2024. The in-toto vetted catalog of predicate types has been actively expanding through 2024–2025. Croissant 1.0 stabilized in 2024 and the Hugging Face Hub auto-Croissant integration landed in 2024. The EU AI Office's training-data summary template was published July 2025. ISCC reached ISO 24138:2024 standardization in 2024. Each of these landings made it incrementally easier to build something on top.

Before 2024, you would have been building on at least two layers of pre-1.0 substrate, which means owning the substrate yourself. That's a different project, with a much larger surface area, much greater standards-political overhead, and much longer time-to-first-customer. Now you can build the compiler on top of stable primitives and let the underlying maintainers handle their own evolution. The cost of building has dropped substantially in the last twelve to eighteen months because the puzzle pieces all finally clicked.

This is the most invisible of the reasons. To a casual observer it looks like Sigstore has existed for years, so why hasn't anyone built on it for corpora? The answer is that Sigstore Model Transparency only just composed with the in-toto vetted catalog, which only just composed with Croissant, which only just composed with the EU template. Before the composition, the build was bigger.

There's also a versioning-and-stability point worth making. Building on pre-1.0 substrates means accepting that your code will break when the substrate releases breaking changes. For a cryptographic project where output bytes have to be reproducible forever, building on shifting substrate is operationally hostile — every substrate change invalidates previously-generated artifacts. The fact that all the relevant primitives now have stable v1.0 specs with backwards-compatibility commitments is what makes the build defensible from a maintenance perspective. Earlier than 2024, you'd have been writing migration scripts every quarter.

---

## Reason 5: Every adjacent player is pulled elsewhere

The teams who could most easily extend their existing work into the Attestrum lane each have specific structural reasons not to.

**Sigstore Model Transparency** is the obvious one. Its funding and engineering come from the OpenSSF, Google, NVIDIA, Red Hat, and HiddenLayer. The strategic interest of those companies is in signing model weights — they sell or distribute models (Google's Gemini, NVIDIA's NeMo, Red Hat's enterprise AI products, HiddenLayer's security around model weights). Signing weights protects their model hubs from poisoning attacks and supports their customers' supply-chain due diligence. Cryptographically attesting *training corpora* is a different value proposition. It would expose their own training-data lineage to downstream auditors — exactly the discovery exposure their corporate legal teams are paid to prevent. There is no internal advocate for extending model-transparency into corpus-transparency, because everyone in the room has a reason to leave it for later. Public statements from the maintainers explicitly note datasets as a future direction, but "future" here is doing a lot of work.

**Intel Labs Atlas** is the second. Atlas-cli is closer to Attestrum than the elevator pitch acknowledges — Rust, Apache 2.0, Sigstore/Rekor integration, datasets included in scope, DoNotTrain opt-out assertions. But Atlas's center of gravity is the model lifecycle and runtime pipeline, not the corpus compile. Intel's strategic interest in the project is TDX hardware attestation — TDX is Intel's product. The Atlas paper's lead case study is BERT fine-tuning with TDX. Datasets in Atlas are documented but not Merkle-committed; opt-out assertions exist but aren't tied to a corpus-level inclusion proof. The corpus-compile pivot would mean rebuilding the project around a different center of gravity, which Intel's research org has no strategic reason to fund. Intel doesn't sell training-data attestation. It sells CPUs with attestation features.

**OWASP AIBOM** is the third. It's the right audience-fit but the wrong governance model for speed. OWASP projects are volunteer-paced, with weekly community meetings, consensus-based decisions, and no funded developer team. Adding Sigstore signing, an in-toto attestation layer, a Merkle commitment over training data, and inclusion-proof primitives would be a year of work for a full-time engineering team. For OWASP it would be three years. By then the standard is set. Also: AIBOM's value proposition is *visibility* (here's what's in the system), not *cryptographic proof* (here's how you verify it). Those are different deliverables for different buyer needs, and AIBOM's audience hasn't asked for the second one yet.

**DataTrails** is the fourth. Pre-acquisition, DataTrails was the most plausible commercial entrant — they had patents, a SaaS motion, and explicit positioning around training-dataset lineage. The OnID acquisition in August 2025 pulled them toward biometric identity verification, banking compliance, and iGaming. Post-acquisition product roadmaps typically take twelve to eighteen months to stabilize, and the resulting roadmap rarely reaches back to the pre-acquisition core market with the same intensity. They'll show up as competition again in 2027, possibly, but not as the original DataTrails — and not on the same technical substrate (blockchain rather than Sigstore), which limits how directly they can compete in the Sigstore-native ecosystem.

**Hugging Face** is the fifth. They have the audience, the distribution, the Hub, and the existing Croissant integration — they're the natural distributor of any provenance standard for AI datasets. But Hugging Face's strategic position is platform neutrality. Building a native attestation system would push them into picking a cryptographic substrate (Sigstore? something else?) and a predicate format (in-toto? C2PA?). Their better play is to wait for the ecosystem to converge on one and integrate it, rather than spend their own engineering capital building something that might lose the standards fight. So they're a natural distribution partner and acquirer, not a builder.

**Chainguard** is the sixth, and the one most often overlooked. Chainguard is the commercial home of Sigstore expertise — most of the senior Sigstore maintainers work there. Technically they could extend their offering into AI corpus attestation. But Chainguard's commercial story is enterprise container security and software supply-chain. Their sales motion targets DevSecOps buyers at large enterprises. Pivoting into AI corpus attestation would mean rebuilding their go-to-market around AI compliance buyers, which is a different sales motion with different customer personas. Strategically they'd rather partner with someone in the AI compliance space than build it themselves.

The pattern across all six: the teams who could most easily build this each have a structural reason not to build it themselves. The gap they leave is exactly the shape of an independent project that doesn't carry their constraints. Each of them has good reasons. The reasons collectively produce a vacuum.

---

## Reason 6: The skill combination is unusually narrow

You need, in approximately equal measure, four distinct kinds of literacy:

**Software supply-chain cryptography** — Sigstore (Fulcio, Rekor, TUF), in-toto Statement and Predicate format, SBOM standards (SPDX 3.0, CycloneDX 1.6), DSSE wrapping, reproducible build discipline, Merkle tree variants (binary, sparse, sorted with adjacent-neighbor non-inclusion proofs), hash function selection and migration paths, certificate transparency design patterns. The people who know this well are at Chainguard, Sigstore, the Linux Foundation, npm/PyPI security teams, or a handful of CNCF projects.

**AI data-ecosystem mechanics** — Hugging Face Hub APIs, dataset card conventions, Croissant JSON-LD, Parquet column statistics, Arrow IPC formats, the practical realities of TB-scale corpora, perceptual hashing for images and audio, MinHash and SimHash for near-duplicate text, ISCC (ISO 24138:2024) as an emerging content-identification standard. The people who know this well are at Hugging Face, MLCommons, Kaggle, AI2, or in dataset-engineering roles at frontier labs.

**EU AI regulation** — Article 53, Attestrum XI, the GPAI Code of Practice, the AI Office's training-data summary template, Recital 107, Article 50 (content labeling) as distinct from Article 53 (training summaries), the CDSM (Copyright in the Digital Single Market) directive Article 4(3) on text-and-data mining opt-outs, the relationship between the AI Act and existing copyright instruments, the operational details of EU enforcement (national competent authorities, the AI Office's role, fine calculation under Article 99). The people who know this well are EU-based AI policy researchers, IP lawyers at firms with AI practices, and a small number of in-house regulatory leads at frontier labs.

**Open-source CLI shipping discipline** — Rust workspace organization, cross-platform CI for byte-deterministic outputs, semver and predicate versioning practice, OSS community building, package distribution (crates.io, Homebrew, system packages), the operational details of a project that has to be drop-dead reproducible across environments, documentation patterns that work for a developer-tool audience.

The intersection of all four is small. Most candidates have one of them deeply, perhaps two. The intersection of three is rare. The intersection of all four is roughly a few dozen people globally, and most of them are currently employed by Chainguard, Sigstore maintainers' day jobs, AI labs' compliance teams, or specialized law firms. There's no natural pool of unaffiliated specialists for a recruiter to draw from. A founder approaching this either has to be one of those people themselves, or has to find a co-founder who is, which is the kind of co-founder search that takes a year.

A second skill barrier worth flagging: this is not a place where you can paper over expertise gaps with a fast LLM assistant. The cryptographic determinism requirements, the regulatory specificity, and the predicate-versioning-frozen-on-first-use discipline mean errors are very expensive. An LLM assistant accelerates implementation within a well-bounded design, but the design judgment has to be human and has to be right. So even teams that could in theory hire generalists and let them learn on the job find that the learning curve eats most of the calendar before they reach a working v0.1.

---

## Reason 7: The fashionable story crowded it out

The 2023–2025 venture pattern for AI startups was either "infrastructure that frontier labs and large enterprises pay for" or "vertical AI application with rapid ARR growth." Both stories had legible exit math. Infrastructure plays like Anyscale, Modal, Together, Pinecone, Weaviate, LangChain, LlamaIndex. Vertical apps like Cursor, Hebbia, Glean, Harvey. The pitch decks all rhymed. The customer logos all pointed at the same handful of buyers.

A patient, Apache 2.0, open-source compiler that serves a small willing audience and waits for a regulatory deadline to bite, with a 3–5 year acquisition-by-foundation-or-platform timeline — that doesn't pattern-match to either fashionable story. It looks like a 2017-era thesis. "Sigstore for X." "Snyk for Y." "Kong for Z." Open-source substrate plays with healthy-business unit economics and a long arc. Those theses got built before 2018 (Sigstore itself, Chainguard's predecessor, the early in-toto work) and then mostly fell out of vogue as the venture market shifted toward SaaS-with-AI and high-ACV enterprise sales.

The people who most naturally would have built "Sigstore for training corpora" were also the people most likely to have been hired into Chainguard, joined Sigstore as maintainers, or moved into AI labs' security teams between 2020 and 2024. The unaffiliated pool of "open-source supply-chain crypto founders looking for their next thing" is small, partly because the previous wave got absorbed into existing successful companies.

Plus: the Elastic/MongoDB license drama of 2018–2019 made VCs cautious about open-source business models. The investor question "what's your moat if AWS forks you?" became a routine objection. Founding a new company around an Apache-2.0 CLI got harder to fund in 2020–2024 even when the underlying technical merit was clear. Not impossible — Chainguard itself raised on this thesis — but harder than vertical AI, and the easier money won.

There's a media-attention layer too. The AI infrastructure narrative that captured tech press in 2023–2025 was about scale, capability, and frontier labs. Provenance was a niche topic that landed in regulatory press, IP-law commentary, and supply-chain security newsletters — not in the tier-1 outlets that influence founder pattern-matching. So even founders who could have seen the gap didn't, because the gap wasn't in their reading list.

---

## Reason 8: Cryptographic determinism is harder than it looks

This is the one most likely to be underweighted by analysts looking at the gap from outside.

Producing the same Merkle root from the same training corpus on different machines, different operating systems, different Rust toolchain versions, different library versions, different filesystem orderings — that's not a checkbox. It's a year of CI work and a permanent ongoing tax on every code change. The classic determinism bugs are well-documented:

HashMap iteration order. System clock leakage into metadata. Compiler version differences in code generation that affect serialization output. Filesystem-dependent file ordering. Locale-dependent string sorting. Floating-point nondeterminism in any analytical pass. Parallelism-induced ordering nondeterminism. Path-separator differences across operating systems. UTF-8 normalization differences across libraries. Trailing-newline handling differences. Tar/zip metadata leakage (mtimes, uids, gids). Compression algorithm differences across versions of the same library.

Each one of these has been the source of expensive supply-chain bugs in real production systems. Debian's reproducible-builds effort took years and is still ongoing. NixOS treats determinism as a first-class property and still finds drift bugs in its packages. Bazel and Buck have entire subsystems devoted to deterministic build modes.

For Attestrum, the cost of getting determinism wrong is unusually high because the output is cryptographic. A non-deterministic Merkle root isn't a bug, it's an attack vector — if the same corpus produces different roots on different machines, then any verifier who computes the root themselves will reject manifests they should accept. The trust assumption inverts. You'd have to ship a "canonical builder" image that everyone has to use, which destroys the open-verifiability story. The whole point of the project is that anyone can verify; without determinism that property dies.

The way the project handles this — four-target cross-platform CI proving byte-identical output, sorted lists everywhere, explicit ban on wall clocks and HashMaps and locale-dependent operations, predicate versioning that's frozen-on-first-use, diagram-first discipline that catches design-level non-determinism before code is written — is a discipline that takes engineering culture to maintain, not just a one-time setup. Most engineering teams don't have that culture. The teams that do have it (Linux Foundation projects, Bazel core, NixOS, reproducible-builds.org) are mostly building tools for compiling software, not corpora.

The result is that even teams who could in principle build the Merkle-over-corpus primitive get the cryptographic determinism wrong in subtle ways, ship something that "works on my machine," and then the project quietly fails to be useful for the open-verification use case that was the whole point. The graveyard of attempted academic prototypes (visible in arXiv papers from 2022–2025) is partly explained by this. Researchers describe the cryptographic construction correctly in the paper but ship reference implementations that aren't actually deterministic in production.

This is the one structural reason that doesn't relax for anyone over time. It's a permanent skill barrier. Whoever ships this is the team with the determinism culture, not the team with the cryptographic theory. Most teams have one. Few have both.

---

## The unclaimed primitive nobody is targeting

Layered on top of all eight reasons is a specific technical observation: even among the projects that exist in the adjacency, none of them target the corpus-level inclusion/non-inclusion proof primitive.

**Sigstore Model Transparency** produces inclusion proofs, but they're inclusion proofs in the *transparency log* — they prove that a signing event was recorded in Rekor at a certain time, not that a particular training document was part of a corpus. That's a different cryptographic object answering a different question. The Rekor inclusion proof tells you "this signature was logged at time T." It doesn't tell you "this document is in the training corpus."

**Atlas** produces C2PA manifests with cryptographic hashes, but a C2PA manifest is structurally a flat or shallow tree of assertions about an asset, not a deep Merkle commitment over millions of corpus documents with efficient inclusion/non-inclusion proofs. The C2PA design optimizes for "this image came from this camera through this editing chain," not "this document is one of nine million entries in a sorted Merkle tree."

**OWASP AIBOM** produces CycloneDX inventories, which have no cryptographic structure to prove inclusion or non-inclusion. The output is human-readable and machine-parseable, but there's no proof attached — you trust the publisher's claim that the inventory is accurate.

**DataTrails** (now OnID) produces blockchain-anchored audit trails, but the lineage records are about transactions ("at time T, party P claimed fact F"), not about corpus contents at the document level.

**The Glasgow paper "Attesting LLM Pipelines"** (accepted at LLMSC 2026) describes verifiable training and release claims at the pipeline level, not the corpus level. **The Composable Attestation paper** from March 2026 describes the cryptographic framework abstractly, not a corpus-specific implementation with shipping code.

**The IETF draft-sharif-ai-model-lifecycle-attestation-00** describes Merkle trees for corpus attestation as part of a broader lifecycle attestation framework, but the draft is individual (single author, no working group adoption) and ships no reference implementation.

So when a rightsholder asks "did your AI train on this specific document of mine?", or when a regulator under Article 53 asks "can you cryptographically demonstrate the inclusion or non-inclusion of this work?", or when a plaintiff asks "produce the corpus-level proof that this work was or was not part of your training set" — nothing in production answers that question directly. The closest thing is "we have a transparency log entry for our model weights" or "we have a CycloneDX inventory of components." Neither of those is the primitive being asked for.

That gap is the actual wedge. The other infrastructure — Sigstore signing, in-toto Statements, Croissant cards, Article 53 summaries — is necessary supporting infrastructure but isn't itself novel. The corpus-document inclusion/non-inclusion proof under a Sigstore-signed Merkle commitment is the thing that's both novel and missing, and it's the primitive that satisfies the actual question rightsholders and regulators are asking.

The pitch's `attestrum prove` command targets this primitive directly. That's the headline. Everything else is supporting structure that makes `attestrum prove` credible and useful.

---

## Why the window is real and narrow

All eight reasons explain the past. What matters for the founder is whether they're still binding.

The obvious-buyer-opposed reason is still binding for frontier labs and always will be. But the willing-transparent-middle alternative has become large enough to support a healthy business. So the reason changed shape, not magnitude.

The actual-buyer-didn't-exist reason has stopped binding. The audience now exists, has explicit transparency commitments, publishes on a visible rhythm, and is funded enough to pay tier-2 prices.

The regulatory-deadline-hasn't-bitten reason stops binding on August 2, 2026. That's a hard date. After that the panic-procurement window opens and lasts roughly 12–18 months.

The standards-substrate-only-just-composed reason has stopped binding. The pieces are stable. Building on them is now an integration project, not a substrate project.

The adjacent-players-pulled-elsewhere reason is currently still binding, but it has a half-life. Sigstore Model Transparency will probably add a datasets track within twelve to eighteen months. Atlas might pivot toward corpus-first work if Intel reorients. Hugging Face will eventually pick a provenance standard. Every month that goes by, the probability that one of these incumbents enters the lane increases.

The skill-combination reason is still binding in general, but it's not binding for any specific founder who happens to have the combination. It's a barrier to the market clearing, not a barrier to a specific entrant.

The fashionable-story reason is relaxing as the AI infrastructure thesis matures. Open-source substrate plays are becoming fundable again as VCs absorb that the vertical AI app market is saturated.

The cryptographic-determinism reason is still binding, and the project's investment in determinism CI is the technical moat. That one doesn't relax for anyone — it's a permanent skill barrier, and either you have the discipline or you don't. This is also why the moat is real: even if Sigstore Model Transparency adds a datasets track tomorrow, getting cross-platform byte-determinism right on day one is unlikely. The first version of any new entrant will have determinism bugs that take quarters to find and fix.

Composing the changes: of the eight reasons, three have substantially relaxed in the last two years (audience, substrate, fashion), one is relaxing on a hard date ten weeks out (regulation), one has changed shape (buyer), two are currently still binding but have a finite half-life (adjacent players, skill combination), and one is permanently binding (determinism, which is also the moat).

The composition produces a window that is real, dated, and narrow. The reason nobody has built this yet is mostly that the conditions only just composed. The reason it has to ship in the next ninety days is that those same conditions are pulling everyone else in too. Sigstore Model Transparency's "datasets are a future direction" remark is a public roadmap admission. Intel Labs Atlas already touches datasets and DoNotTrain assertions. The OWASP AIBOM team has weekly meetings. Hugging Face is watching for a credible standard to emerge that they can integrate. The race hasn't started yet, but the starters' marks are visible.

The path to "we built this in time" runs through six sprints, three design partner conversations, an in-toto vetted catalog submission, a Hugging Face integration, and an Article 53 enforcement-date demo. The path to "we missed it" runs through any of those slipping by more than four weeks. The window will close. It just hasn't closed yet.

---

## Explained like you're in elementary school

> Same audience as `elementary-explain.md` — friendly, no jargon, concrete metaphors. If you haven't read that one first, the backpack-and-sticker setup is in there.

You know how Attestrum is a sticker-machine for backpacks? Here's why no other kid in the whole school has built one yet.

### The popular kids really, really don't want a sticker-machine

The biggest kids in school — the ones who built the biggest AI machines — definitely don't want a sticker-machine. They have LOTS of stuff in their backpacks, and some of it might be borrowed from other kids who never said it was okay. If the teacher ever asks "what's in your backpack?", the popular kids would much rather say "oh, lots of stuff, hard to remember exactly." A sticker that lists every single item would make that vague answer impossible.

Right now some of those popular kids are in actual trouble with the principal's office, because other kids' parents are asking "did you take my kid's stuff and put it in your backpack?" The popular kids are paying their helpers (the lawyers) lots of allowance to keep the answer fuzzy. A perfect, bright, magic sticker that lists every page in the backpack would make the helpers' job impossible. So the kids with the most lunch money are the kids who least want this thing to exist.

If you're a new kid thinking about building a sticker-machine, you naturally look at the popular kids first because they have the most money. But they don't want what you're selling. They want the opposite. Once you figure that out, most new kids give up and go work on something else, like a faster scooter.

This means: the gap exists not because nobody noticed it, but because almost everyone who noticed it walked away once they saw the popular kids wouldn't pay.

### The kids who DO want a sticker-machine just got to school

There are some kids at school who genuinely want a sticker-machine. They actually *like* showing the teacher what's in their backpacks. Their names are AI2, EleutherAI, Pleias, Black Forest Labs, and Mozilla. But here's the thing — these kids only just transferred to this school. Mozilla's transparent-AI group only started this year. EleutherAI's Common Pile only came out this year. Pleias's big shared backpack only finished getting packed last year.

Before all those kids transferred in, there wasn't really a group of "kids who want sticker-machines." There were one or two scattered around, but you couldn't point at a lunch table and say "those kids would buy a sticker-machine." Now you can. The lunch table just formed.

There's also a second group worth knowing about. Some kids in Europe who aren't super big or super popular, but they don't have lots of lawyer-money either, so they can't really argue "what's in my backpack is a secret." Those kids would actually be relieved to have a sticker. They could just put up the sticker and say "here, this is what's in there, leave me alone." Those kids exist now too. They didn't really exist three years ago in the same way.

### The school rule that requires backpack-checks doesn't kick in until August

There's a new school rule from Europe that says big AI machines have to publish a summary of what's in their training backpacks. The rule is real, and the punishment for breaking it is real (fines of millions of dollars). But the punishment doesn't actually start until August 2, 2026 — which is ten weeks from now.

This is normal for school rules. Everybody knows a rule is coming. Most kids wait until someone actually gets sent to the principal's office to start really paying attention. Until the first kid gets in trouble, everyone just kind of shrugs and says "we'll deal with it later."

So the urgency for buying a sticker-machine hasn't really arrived yet. It arrives August 2. Most sticker-machine builders would wait until then to start. We're starting now because by the time August arrives, the first kid to have a working sticker-machine gets to write the "this is how stickers work" rulebook forever, and you can't win the rulebook race if you start when everyone else does. You have to already be running when the gun goes off.

### The tools to build a sticker-machine only just got finished

To build a sticker-machine, you need a bunch of special tools. A special pen that makes magic fingerprints (BLAKE3). A special stamp that combines fingerprints into one big stamp (Merkle tree, RFC 6962). A special signature pad that proves who made the sticker (Sigstore). A special rulebook for what stickers say (in-toto). A special way to write the sticker's contents so robots and humans can both read it (Croissant). And a special form the school principal wants (the EU template).

Each of these tools existed for a while, but they only just finished being made compatible with each other. BLAKE3 stabilized a few years ago. Sigstore got its v1.0 stamp. The in-toto rulebook just got updated. Croissant 1.0 came out. The EU principal's form just got published last summer.

Before all these tools were ready, building a sticker-machine meant ALSO building your own pen, your own stamp, your own signature pad, AND your own rulebook. That's way too much work for one kid. Now you just snap the tools together and focus on the sticker-machine itself. The pieces all click now. They didn't click two years ago.

### Every kid who could easily build one is busy with something else

There are some kids who already have part of a sticker-machine — and each of them has a reason they can't easily turn what they have into the thing we need.

The kids who built **Sigstore Model Transparency** can already sign the AI machine itself. Their stickers are for the *machine*, not for what's inside the backpack. They could extend their work to backpack-content stickers, but their parents (Google, NVIDIA, Red Hat) all run big AI machines themselves and don't want backpack-content stickers to exist for their own backpacks. So those kids aren't going to push that direction even though they could.

The kid who built **Atlas** is at Intel. Their sticker-machine is good but it's built around a different stamp system, and their parents care most about a special hardware-fingerprint feature on Intel's chips. They're close to the right place but pointed slightly the wrong way, and their parents pay them to stay pointed that way.

The kid who built **OWASP AIBOM** is doing volunteer work after school. They make a really good list of what's in the backpack, but their list doesn't have any signatures or magic stamps. Adding those would take them a year. By then someone else has won the rulebook.

The kid who built **DataTrails** moved to a different school last August. They used to make stickers for backpacks. Now their new school wants them to make fingerprint-readers for the front door instead. They might come back someday but not soon, and not with the same setup.

The kid who runs **Hugging Face** has the lunchroom where everyone trades backpacks. They could put up a sticker-stand of their own, but they'd rather wait for someone to come build the best sticker-stand and then invite them to set up in the lunchroom. That's actually smarter for them — they get the best stickers without having to invent them.

The kids at **Chainguard** know how to build all the tools, but they're busy making stickers for software backpacks, not training backpacks. They could do it. They're doing other things.

Every kid who could build a sticker-machine has a reason they're building something else instead. That's not a coincidence — it's six different specific reasons that all line up into the same vacuum.

### Building a sticker-machine needs three different school subjects

To build a really good sticker-machine, you have to be good at three different school subjects at the same time.

**Chemistry class** (the magic math of fingerprints and stamps). You need to know exactly how the fingerprint pens work, and how the magic stamp combines fingerprints, and what happens if you put them in the wrong order, and how to make sure two different computers always get the exact same stamp.

**Art class** (the look of the sticker and what shape it has). You need to know how AI machines actually pack their backpacks — what kinds of pages they put in, how they label them, how the big stack of papers gets organized, how big a really big backpack actually is.

**Civics class** (the school rules). You need to know exactly what the principal wants on the sticker, what the rules say can be a secret, and what every other school in Europe is doing.

Most kids are really good at one of these. Some are good at two. Almost nobody is good at all three. The kids who are good at chemistry are usually working in the cryptography lab. The kids who are good at art are usually working at Hugging Face or MLCommons. The kids who are good at civics are usually lawyers. To build a sticker-machine you need someone (or a small team) who can do all three at once, and people like that are rare and usually already busy doing something else.

### Everyone wanted to make video games instead

For the last few years, the cool kids in tech school all wanted to either:

1. Sell expensive things to the popular kids (the AI labs)
2. Build a new shiny app and become rich very fast

A free sticker-machine for a small group of careful kids isn't either of those. It looks more like the old-fashioned "make a tool everyone in the world uses for free and slowly become important" story — which used to be cool but went out of fashion around 2020 when everyone realized vertical AI apps could grow much faster.

So even kids who could see the sticker-machine gap clearly mostly chose the cooler stories. The kids who built tools like Sigstore back in 2018 got hired by Chainguard and Sigstore itself and are doing well. The pool of "available kid who wants to build a free sticker-machine" got pretty small. The grown-ups who give kids money for projects (the VCs) also stopped funding the free-tool kind of project, because they got worried that bigger kids would just copy the free tool and not pay anything. So even kids who wanted to build sticker-machines had a harder time getting the money to do it.

### Making the same sticker twice is really, really hard

Here's the secret reason this is harder than it looks. Remember the same-sticker rule? Two kids with the EXACT same backpack contents have to get the EXACT same sticker. Down to the last letter.

This sounds easy, but computers are sneaky. They put things in different orders depending on the day. They sneak the current time into stuff. They sort lists differently depending on what language the computer is set to. They use different math when they're going fast versus slow. They put extra invisible characters in places you can't see. They name files differently on Apple computers than on Windows computers.

Getting two computers to produce truly identical stickers is a year of really careful work. You have to ban a bunch of normal things that computers do. You have to test on four different kinds of computers every time you change anything. You have to be paranoid about every single byte of output.

Most kids who try to build a sticker-machine skip this hard step, ship something that works on their own computer, and then the sticker comes out different on someone else's computer, and the whole point of being able to verify the sticker breaks. The kids who know how to make truly identical things across different computers mostly work on Linux or NixOS or Bazel — they're building tools for compiling code, not for compiling backpacks. So when other kids try to make a corpus sticker-machine, they often get this step wrong and the project quietly stops working.

This part doesn't get easier over time. It's a permanent skill thing. Either you have the discipline or you don't. That's actually good news for us — even if a big kid copies what we're doing later, they probably won't get the same-sticker rule right on their first try.

### Nobody's even tried to do the trickiest part

Even the kids who are doing close work haven't tried the trickiest sticker — the "YES this paper is in my backpack" sticker, or the "NO this paper isn't in my backpack" sticker.

Sigstore Model Transparency does YES stickers, but for the *moment they signed the manifest*, not for the contents of the manifest. (It says "yes, we signed this on Tuesday." It doesn't say "yes, this poem is in our backpack.")

Atlas makes good sticker manifests but doesn't make per-page yes/no proofs.

OWASP AIBOM makes a list of what's in the backpack but doesn't sign it or make it provable.

DataTrails makes stickers about *changes to* the backpack, not about its *contents* page by page.

So when an artist asks "is my painting in your AI's backpack?", or when the school principal asks "can you prove this poem isn't in your backpack?", nobody has the right sticker. That specific trickiest sticker is the actual prize nobody has gone after yet. It's the headline thing Attestrum is building. The other stuff (signing, Merkle, Croissant, Article 53) is the supporting cast. The yes/no sticker is the star of the show.

### Why this is the perfect moment

Here's the thing. ALL of these reasons used to be true. Most of them are stopping being true at the same time.

The kids who want stickers just transferred to school. The school rule kicks in in ten weeks. The tools finished being made compatible. Vertical AI apps are getting boring so kids are starting to look at older stories again. And the popular kids' situation hasn't changed, but now there are enough other customers that you don't need the popular kids anymore.

The kids who are positioned to easily build one (Sigstore Model Transparency, Atlas, OWASP AIBOM) are still pointed elsewhere, but they could turn at any moment. Every month that passes, one of them might wake up and say "we should do this." So the window is real — but it's also narrow.

If we wait until next year, Sigstore Model Transparency probably adds a datasets track and writes their own version. Atlas probably gets nudged toward corpus work. Hugging Face probably picks a standard and integrates it. The window quietly closes.

If we ship in ninety days and get the rulebook accepted into the in-toto vetted catalog and get a few willing-transparent kids using us before August 2, we win. The first sticker-machine with real users on the first day of the new rule becomes the answer to "how do you make stickers?" forever, because nobody re-evaluates compliance tools when they're panicking. They just buy whatever everyone else is buying.

That's why we're building now, in ninety days, instead of waiting. The reasons nobody built this yet are also the reasons the race is about to start. We just want to be running before everyone else lines up at the starting line.

The gap is real. The window is real. The deadline is real. The race hasn't started yet — but you can hear the starter loading the gun.

---

*Last updated: end of Sprint 3. Companion to `elementary-explain.md` and `elevator-pitch.md`.*
