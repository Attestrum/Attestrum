This dataset is **databricks-dolly-15k**, an open-source corpus of ~15,000 instruction-following records written by **Databricks employees**.

- **Source:** [`databricks/databricks-dolly-15k`](https://huggingface.co/datasets/databricks/databricks-dolly-15k), config `default`, split `train` (Hugging Face).
- **Original work:** © Databricks, Inc., licensed under [CC BY-SA 3.0 Unported](https://creativecommons.org/licenses/by-sa/3.0/). Some records include reference passages derived from CC BY-SA 3.0 Wikipedia.
- **Modifications:** each row was rendered to natural text before sealing — the `instruction`, the `context` block (only when non-empty), and the `response`, separated by single blank lines, one row per leaf. The free-text `category` tag was not included. No content was otherwise added or removed.
- **License of this distribution:** released under the same license — **CC BY-SA 3.0** (ShareAlike) — as required by the source.
