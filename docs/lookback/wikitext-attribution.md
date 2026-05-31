This dataset is **WikiText-103 (raw)**, derived from articles on the English **Wikipedia**, written by Wikipedia contributors.

- **Source:** [`Salesforce/wikitext`](https://huggingface.co/datasets/Salesforce/wikitext), config `wikitext-103-raw-v1` (Hugging Face).
- **Original work:** © Wikipedia contributors, licensed under [CC BY-SA 3.0 Unported](https://creativecommons.org/licenses/by-sa/3.0/).
- **Modifications:** the source text was detokenized to natural English (reversing Moses-style ` @-@ ` and spaced-punctuation artifacts) and segmented into one passage per leaf before sealing. No content was otherwise added or removed.
- **License of this distribution:** released under the same license — **CC BY-SA 3.0** (ShareAlike) — as required by the source.
