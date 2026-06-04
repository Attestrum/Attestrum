# Showcase passages — source & attribution

These five files are **byte-exact leaves** of the published, Attestrum-sealed
**WikiText-103 (raw)** corpus (`Attestrum/wikitext-103-sealed`), each derived
from articles on the English **Wikipedia**, written by Wikipedia contributors.
They exist so the `lookback-prove-examples.yml` workflow can mint a signed
`inclusion-proof/v0.3` attestation for a passage a visitor can recognise, and
so the landing page can demonstrate verifying that proof with stock `cosign`.

- **Source:** [`Salesforce/wikitext`](https://huggingface.co/datasets/Salesforce/wikitext), config `wikitext-103-raw-v1` (Hugging Face), as sealed at [`Attestrum/wikitext-103-sealed`](https://huggingface.co/datasets/Attestrum/wikitext-103-sealed).
- **Original work:** © Wikipedia contributors, licensed under [CC BY-SA 3.0 Unported](https://creativecommons.org/licenses/by-sa/3.0/).
- **Modifications:** the source text was detokenized to natural English (reversing Moses-style ` @-@ ` and spaced-punctuation artifacts) and segmented into one passage per leaf before sealing. The files below are individual sealed passages, copied verbatim; no content was otherwise added or removed.
- **License of these excerpts:** the same as the source — **CC BY-SA 3.0** (ShareAlike).

## Passage → source article

Each file is one sealed leaf; its content is byte-identical to the matched
corpus passage (verified `attestrum prove --against <published manifest>
--unsigned` → `inclusion`, confidence `1.00`). The corpus passage id maps to a
Wikipedia article and paragraph.

| File | Corpus passage id | Source article | Cluster |
|---|---|---|---|
| `passage-01.txt` | `wikipedia://Dwarf_planet#p1` | [Dwarf planet](https://en.wikipedia.org/wiki/Dwarf_planet) | science / nature |
| `passage-02.txt` | `wikipedia://USS_Illinois_(BB-7)#p1` | [USS Illinois (BB-7)](https://en.wikipedia.org/wiki/USS_Illinois_(BB-7)) | history / military |
| `passage-03.txt` | `wikipedia://Hurricane_Omar_(2008)#p1` | [Hurricane Omar (2008)](https://en.wikipedia.org/wiki/Hurricane_Omar_(2008)) | geography / weather |
| `passage-04.txt` | `wikipedia://Valkyria_Chronicles_III#p2` | [Valkyria Chronicles III](https://en.wikipedia.org/wiki/Valkyria_Chronicles_III) | arts / pop-culture |
| `passage-05.txt` | `wikipedia://Loose_(Nelly_Furtado_album)#p1` | [Loose (Nelly Furtado album)](https://en.wikipedia.org/wiki/Loose_(Nelly_Furtado_album)) | arts / music |
