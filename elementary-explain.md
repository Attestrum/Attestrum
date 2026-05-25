# Attestrum, explained like you're in elementary school

A friendly tour of what we're building, why, and why some parts are harder than they look. No jargon. No acronyms (well, fewer acronyms).

---

## The Big Picture

Imagine a kid brings a giant backpack of homework to school. The teacher wants to know what's inside without dumping it on the floor.

**Attestrum is a machine that looks through the backpack and makes a special sticker.** The sticker says "this backpack has 100 math worksheets, 50 reading pages, and zero comic books." It has a magic stamp on it — if anyone tries to sneak a comic book in later, the sticker turns red. Anyone in the world with the right magnifying glass can check the sticker, even people who've never met you.

That's Attestrum in one sentence: a sticker-making machine for backpacks.

---

## Who's the Kid? Who's the Teacher?

In real life:

- **The kid** is a company that trains AI (like ChatGPT or Stable Diffusion). They have a huge pile of text and pictures they used to teach their AI.
- **The teacher** is everybody else: regulators in Europe asking "what did you train on?", artists wondering "did you use my paintings?", customers asking "is this AI made from stolen stuff?".
- **The backpack** is what we call a "training corpus" — the giant pile of documents the AI learned from. Sometimes millions of files. Sometimes billions.
- **The sticker** is what we call a "provenance bundle" — a small, signed file that summarizes the backpack without you having to look at every page.

The kid wants to PROVE the contents without having to invite everyone to dig through the actual backpack (which might be 10 terabytes, which is like 10,000 thumb drives).

---

## What's Actually In the Backpack?

For an AI company, the backpack might have:

- Web pages they downloaded
- Books they bought a license to
- Images from photographers who said "yes please use my photos"
- Videos with subtitles
- Computer code from open-source projects

For each piece of paper in the backpack, we want to know:
- Where did it come from? (a website? a book? a license deal?)
- Did the owner say "okay to use" or "please don't"?
- When did you grab it?
- What kind of thing is it? (text? picture? audio? video?)

The sticker captures all of that for the WHOLE backpack at once.

---

## How the Sticker Works (The Magic Stamp)

Here's the magic-stamp trick.

We take every single piece of paper in the backpack. For each one, we compute a tiny "fingerprint" — like a magic 32-letter code that's unique to that exact piece of paper. Change even one letter on the paper and the fingerprint becomes a completely different 32-letter code.

Then we take all those fingerprints, line them up in a special order, and combine them in pairs, then combine the pairs in pairs, all the way up until we have ONE final 32-letter code that summarizes the whole backpack.

That final code is called the **Merkle root** (named after Ralph Merkle, the guy who invented the trick in 1979). It's the magic stamp on the sticker.

If even one piece of paper in the backpack changes — one typo, one extra word — the Merkle root changes completely. Nobody can fake it. They'd need every original piece of paper to make a real one.

The fingerprint algorithm we use is called **BLAKE3**. It's a math recipe that turns any amount of data into a 32-letter code. It's been studied by cryptographers and nobody has found a way to break it.

---

## The Library (Content-Addressed Storage)

Where do we put the actual papers? We have a special library called the **CAS** (content-addressed store).

Normal libraries file books by their TITLE. Our library files books by their CONTENT. Every paper gets stored in a folder named after its fingerprint. So a paper with fingerprint `abcd1234...` lives in a folder called `ab/cd/abcd1234.bin`.

Two cool things happen because of this:

1. **No duplicates ever.** If two papers have identical content, they have identical fingerprints, so they go in the same folder. We only store one copy.
2. **You can't lie about what's in a paper.** If a paper claims to be `abcd1234...`, you can always recompute its fingerprint and check. If the fingerprint doesn't match, you know someone messed with it.

It's like a library where every book's call number IS what's printed inside the book. You can't put a fake book on the shelf because the call number would be wrong.

---

## The Same-Sticker Rule (Determinism)

Here's a rule we're really strict about: if two different kids have the EXACT SAME backpack contents, they MUST get the EXACT SAME sticker — down to the very last byte.

This sounds obvious but it's actually hard to do. Computers do things in slightly different ways on different days. Maybe one computer puts items in a list in one order, and another computer puts them in a different order. Even though both are "right," they produce different stickers.

We had to be careful about lots of tiny things:
- Never use a wall clock (timestamps change every second)
- Never use the kind of dictionary that scrambles its entries (called a "HashMap")
- Always sort lists into a fixed order
- Always use the same compression settings
- Never let the computer's CPU make choices that depend on speed

We test this by running our sticker-machine on FOUR different kinds of computers (different Apple chips, different Linux versions, different libraries) and making sure all four produce the SAME sticker bit-for-bit. If even one byte is different, our test breaks.

This is called **determinism**, and it's one of the harder parts of Attestrum.

---

## Three Kinds of Stickers (The Predicates)

We don't just make ONE kind of sticker. We make three kinds:

1. **The "Here's My Whole Backpack" sticker** (called `training-corpus`). This summarizes everything in the backpack. Most stickers are this kind.

2. **The "Yes This Paper Is In My Backpack" sticker** (called `inclusion-proof`). If someone says "is your AI trained on this specific photo?", you can produce a small proof: "yes, here's the math showing this exact photo is in my backpack."

3. **The "No This Paper Is NOT In My Backpack" sticker** (called `non-inclusion-proof`). The opposite. "No, I did not train on this poem. Here's the math showing it's not in my backpack."

The second and third kinds are clever because they don't reveal what ELSE is in the backpack. You can prove a single paper is or isn't there without showing the other million papers.

Each kind of sticker has a "rulebook" that lives at a special web address:
- `attestrum.com/attestation/training-corpus/v0.1`
- `attestrum.com/attestation/inclusion-proof/v0.1`
- `attestrum.com/attestation/non-inclusion-proof/v0.1`

The rulebook says what fields the sticker has and what they mean. Anyone in the world can read the rulebook and understand our stickers.

---

## Frozen Rulebooks (PROTECTED Systems)

Here's a rule that makes us draft things SUPER carefully.

Once we publish a rulebook at `attestrum.com/.../v0.1`, we can NEVER change it. Not ever. Not even to fix a typo.

Why? Because the moment we publish it, someone might use it to make a real sticker for a real backpack. If we then change the rulebook, their sticker stops being readable — like changing the rules of chess after a game has started.

We can make a NEW rulebook called `v0.2` later. But `v0.1` is frozen forever the second anybody uses it.

We have some other things that work the same way:

- **The way we compute the magic stamp** (the Merkle math). Frozen since Sprint 2. Change it and every old sticker becomes meaningless.
- **The way we organize the library** (the CAS folder layout). Frozen since Sprint 2. Change it and old libraries can't be read.
- **The shape of the sticker file** (the Parquet schema). Frozen since Sprint 3. Change it and old stickers can't be opened.

When we touch any of those, the rule is: stop. Get the founder's explicit permission. Write a migration document. Add a special footer to the commit that says "Protected-system-change: approved-by=Austin Munday".

This is why we move slowly on the foundations. Move fast on the leaves, slow on the trunk.

---

## Why We Draft Carefully (Cross-Checks)

When we're about to design something that becomes frozen forever, we don't just decide alone. We ask a SECOND smart helper to look at the same problem independently and see if they reach the same answer.

The second helper is another AI (a different one — GPT-5.5 Pro). We give them the problem WITHOUT showing them our answer. They think about it from scratch. Then we compare.

If both helpers come to the same conclusion, that's strong evidence the design is right.

If they disagree, that's a flashing red light. Stop. Figure out who's wrong. Sometimes the second helper catches something we missed.

We did this once already in Sprint 3 for the sticker-file shape. The second helper agreed with us on most things but recommended being MORE conservative about five technical choices. We adopted all five recommendations. Made the foundation more durable.

We'll do it again in Sprint 4 for the three sticker rulebooks. Those are URLs we can never change, so we want to be triple-sure they're right.

---

## The Homework Form (Article 53 / EU Regulation)

The European Union passed a law in 2024 called the **AI Act**. Part of it (Article 53, paragraph 1, letter d) says: every company that ships a big AI model in Europe has to publish a summary of what they trained the AI on.

The EU wrote a template. Every AI company has to fill out the same template. It asks things like:
- Who are you?
- What's your AI called?
- How big is your training data (roughly)?
- What kinds of content (text? images? audio?)?
- What licenses?
- What rules did you follow about people who said "don't use my stuff"?
- How can copyright holders contact you?

The deadline is **August 2, 2026** — about 9 months from now. Companies that don't comply can be fined up to €15 million or 3% of their global revenue, whichever is bigger.

Our sticker has all the information needed to fill out this template. So we wrote a thing called `attestrum emit article-53` that takes a sticker and automatically produces the filled-out form (as a PDF and a JSON file). That ships in Sprint 5.

There's a second, more detailed form called **Attestrum XI** that's required for the biggest "systemic risk" AI models. We auto-fill what we can; humans have to write the rest (like "describe your organization's structure"). Also Sprint 5.

---

## The Official Club (in-toto Vetted Catalog)

Remember the three rulebooks at `attestrum.com/...`? Right now, those are just files on OUR website. Anyone can read them. But they're not "official."

There's a worldwide community of grown-ups (the **in-toto project**) who maintain a list of "official" rulebooks. If our rulebooks get on the list, every signature-checking tool in the world starts to recognize them automatically.

We can't just ASK to be on the list. The community wants evidence that our rulebooks are actually USED by real people. They want to see:
- Real adopters using our format
- Reasonable design choices
- Independent reviewers saying "yes, this is a sensible spec"

So our path is:
1. **Sprint 4 (now):** publish the rulebooks at `attestrum.com/...`
2. **Sprint 5:** make our software produce real stickers using them
3. **Sprint 6:** do a public demo with real users (Pleias, AI2, a Hugging Face dataset publisher)
4. **After we ship v0.1.0:** wait 3-6 months for real users to generate bundles in the wild
5. **Then:** submit to the in-toto community catalog with evidence of use

Getting onto that catalog is the real prize. It's our **moat**. We don't make money from making stickers — we make the RULES for what a sticker looks like. Anyone who wants to check ANY AI's training data has to use OUR sticker format.

This is the same kind of moat that JPEG has for images, or PDF has for documents. Lots of people make JPEG software. The Joint Photographic Experts Group doesn't get a cut. But everyone has to follow JPEG's rules. That's the strategic position.

---

## Why We Picked This Pitch (Path A)

We originally had a different idea. We were going to sell our sticker-machine to the BIGGEST AI companies — Google, Meta, Anthropic, OpenAI. "Hi, you can prove your training data with our tool!"

We killed that pitch in May 2026 after looking at it honestly. Big AI companies are in court fighting to keep their training data SECRET. They are paying expensive lawyers to argue "we don't have to tell anyone what we trained on." They are NOT going to buy a tool that makes them auditable. They want ambiguity. We were selling clarity. Bad fit.

So we pivoted to **Path A**: serve the companies that ALREADY want to be transparent and don't have a tool. There are six obvious ones:

1. **AI2** (Allen Institute) — they publicly share their training data
2. **Pleias** — small French AI company explicitly built on open data
3. **EleutherAI** — open-source AI research group
4. **Black Forest Labs** — makers of Flux image AI, image-rights-conscious
5. **Mozilla Data Collective** — Mozilla's data-stewardship arm
6. **Hugging Face** — the GitHub of AI, hosting millions of datasets

These groups WANT to be transparent. They just don't have a clean tool. We make the clean tool. They use it. Their bundles get published on Hugging Face. The format spreads.

The Path A pitch document is at `PATH-A-BRIEF.md` in the repo. It explains all of this in more detail.

---

## The Six Sprints (Our Plan)

We're building this in 90 days, split into six "sprints" of two weeks each.

| Sprint | Weeks | What |
|--------|-------|------|
| **Sprint 1** | 1-2 | Set up the workspace + ship the three most important signal parsers (robots.txt, ai.txt, TDMRep) |
| **Sprint 2** | 3-4 | Hash function (BLAKE3), Merkle tree (RFC 6962), data store (CAS) |
| **Sprint 3** | 5-6 | The sticker file format (Parquet manifest), the pipeline that builds a sticker, the CLI subcommands `attestrum build` / `inspect` / `plan` / `merge` |
| **Sprint 4** | 7-8 | Sign the sticker with Sigstore, wrap it in in-toto Statement v1, add `attestrum sign` / `attestrum verify` |
| **Sprint 5** | 9-10 | Fingerprint individual items (so you can prove "yes/no this is in my backpack"), the EU regulatory forms, the Croissant format (for Hugging Face) |
| **Sprint 6** | 11-12 | Publish to Hugging Face for real, run the end-to-end demo on a 5GB dataset, ship version 0.1.0 |

We're at **end of Sprint 3** right now. 241 tests pass. 27 diagrams. 4-target cross-platform CI is green. The sticker-machine works end-to-end except for the signing part (which is Sprint 4).

---

## Draw Before You Code (Diagram-First Discipline)

One unusual rule we follow: **we draw a picture of every part BEFORE we write the code for it.**

Every module, every CLI command, every error path, every multi-party flow — gets a diagram in `docs/diagrams/` first. We use a tool called Mermaid that draws diagrams from text. The diagrams live in the repo alongside the code.

Why? Because:
1. It forces us to think before we type
2. It makes review easier (a human can scan a diagram in 30 seconds; reading the code takes 10 minutes)
3. We can show the diagrams to non-programmers (founder, design partners, regulators) and get feedback
4. It catches confusion early — if the diagram is hard to draw, the code will be hard to write

We even built an automatic checker (the **diagram-linter**) that makes sure every diagram parses correctly, has the right metadata, references real files in the codebase, and that every public thing in the code is mentioned in at least one diagram. If the diagram and the code drift apart, the linter breaks the build.

This is unusual. Most code projects skip this step. We do it because we're building something that needs to be CORRECT (cryptography + regulation = errors are very expensive).

---

## How We Move (Plan-First, Per-Commit Go)

Here's how a normal work session goes:

1. **The agent (that's me, Claude) starts in "plan mode."** I can read files. I can think. I can NOT write code or run commands.
2. **I draft a plan.** What's the goal? What files will I touch? What tests will I write? What could go wrong?
3. **I show the plan to the founder.** They say yes, no, or "change this part."
4. **If yes, the founder says "go" explicitly.** Now I'm out of plan mode. I can write code.
5. **I write the code for ONE commit.** Just one. I run the tests. I make sure everything works.
6. **I show the founder what I built.** They review.
7. **If they like it, they say "go on the next thing."** Back to plan mode. Repeat.

This is slower than just "vibing it out" but produces much higher quality work. We catch design mistakes BEFORE they're committed. The founder stays in control of the direction.

---

## What We've Actually Built (As of Sprint 3 Close)

A real software project with:

- **14 Rust crates** in one workspace (smaller libraries that fit together)
- **7 of them have real code**; the other 7 are stubs waiting for their sprint
- **241 tests** that all pass
- **27 architecture diagrams** that all match the code
- **4-target cross-platform CI** that proves our sticker is identical on every kind of computer
- **An asciinema cast** (a recording of the demo) at `docs/demos/sprint-3.cast` you can play to see the whole thing run
- **The `attestrum` command-line tool** with 4 subcommands: `build`, `inspect`, `plan`, `merge`

What's missing (Sprint 4-6):

- Signing (Sprint 4)
- Verifying (Sprint 4)
- Fingerprinting individual items (Sprint 5)
- Producing the EU regulatory forms (Sprint 5)
- The Croissant + CycloneDX sidecar formats (Sprint 5)
- Publishing to Hugging Face (Sprint 6)
- The static `verify.html` web page that lets anyone verify a bundle from their browser (Sprint 6)
- The takedown ledger (Sprint 6 — what happens when a rights-holder asks you to remove their content)

---

## What "Done" Looks Like (The v0.1.0 Demo)

At the end of Sprint 6, we want to do this demo on a fresh laptop:

1. Download a 5 gigabyte slice of an open-source AI dataset called **Common Pile v0.1** (it's a real public dataset built by EleutherAI + a bunch of universities).
2. Run `attestrum build` on it. Produces a sticker.
3. Run `attestrum sign` on the sticker. Adds the Sigstore signature.
4. Run `attestrum publish` to upload everything to a public Hugging Face dataset repository at `huggingface.co/datasets/attestrum/common-pile-mini-v0.1`.
5. Walk over to a SECOND laptop that doesn't have Attestrum installed at all.
6. Open a web browser. Go to the Hugging Face page.
7. Click "verify." The browser-only verifier (no install needed) walks through the proof and shows green: "this dataset is authentic, here's who signed it, here's when, here's the Merkle root."

If that demo works on a fresh laptop with no Attestrum installed, we've shipped v0.1.0.

---

## Why This Matters

A few converging reasons:

1. **The EU AI Act enforcement starts August 2, 2026.** Big AI providers HAVE to publish training data summaries or face €15M / 3%-of-revenue fines.
2. **Copyright lawsuits are everywhere.** Stable Diffusion, ChatGPT, GitHub Copilot — all in court. Courts want to know what was trained on. Right now nobody has a clean way to answer.
3. **The willing-transparent middle has no tool.** AI2, EleutherAI, Pleias, Black Forest Labs, Mozilla — they all WANT to publish provenance. They're stuck writing one-off README files.
4. **Hugging Face hosts millions of datasets** and has no provenance standard. They want one. The first clean tool to integrate with HF likely wins the format.
5. **The in-toto vetted catalog is the long-term moat.** First-mover with a clean predicate wins the standard. Everyone else has to follow.

If we time it right — ship a clean tool by August 2026, get it into Hugging Face's publish flow, get the in-toto catalog entry within 12 months — Attestrum becomes the default way to ship AI training data with provenance. Forever, basically. Standards don't change easily once they're set.

That's the bet.

---

## Glossary (Quick Reference)

- **Attestrum** — our tool
- **Sticker / Bundle** — the signed proof file Attestrum produces (`manifest.sigstore.json`)
- **Manifest** — the spreadsheet inside the sticker that lists every document
- **BLAKE3** — the fingerprint math (cryptographic hash function)
- **Merkle tree** — the magic-stamp math that combines fingerprints into one root
- **CAS** — Content-Addressed Store, our library where files are filed by their fingerprint
- **Determinism** — every computer making the same sticker for the same backpack
- **Sigstore** — the signature system we use (run by the Linux Foundation, free, public)
- **in-toto** — the standards body for "attestation" formats (the rulebooks our predicates follow)
- **Predicate** — one of our three sticker formats (`training-corpus`, `inclusion-proof`, `non-inclusion-proof`)
- **Article 53** — the EU AI Act paragraph requiring training-data summaries
- **Path A** — our chosen pitch (serve transparent middle), versus Path B (sell to frontier labs, killed in May 2026)
- **Sprint** — a 2-week chunk of work. We have 6 of them.
- **PROTECTED system** — a part of the code that's frozen and can't change without major ceremony
- **Cross-check** — getting a second AI to independently verify our high-stakes design choices
- **Plan mode** — the agent (me, Claude) can read but not write. The founder explicitly lifts this with "go."
- **Diagram-first** — every part of the system gets a picture in `docs/diagrams/` BEFORE the code is written
- **Pre-commit gate** — five checks (format, lint, test, diagram-lint, license-audit) every commit has to pass

---

If you read this and have questions, that's a good sign. The hard parts of this project are the parts that are SUBTLE — easy to get wrong in ways that aren't obvious until you've shipped to a million users. The diagrams + cross-checks + protected-system discipline are how we catch those before they become permanent.

The code is medium-hard. The strategy is high-stakes. The discipline is what makes the two add up to something durable.
