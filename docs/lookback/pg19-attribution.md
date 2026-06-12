This dataset is **PG-19**, a corpus of 28,752 complete books published before 1919, extracted from the Project Gutenberg library by **DeepMind** (Rae et al., 2019, *Compressive Transformers for Long-Range Sequence Modelling*).

- **Source:** [`deepmind/pg19`](https://huggingface.co/datasets/deepmind/pg19) (Hugging Face; data hosted in DeepMind's public `deepmind-gutenberg` Google Cloud Storage bucket), splits `train` + `validation` + `test`.
- **Original work:** the dataset compilation is © DeepMind, licensed under [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). The underlying book texts were published before 1919 and are in the public domain; DeepMind's stated preprocessing stripped Project Gutenberg license boilerplate and replaced certain words with `<DW>` tokens.
- **Modifications:** none. Each book file was sealed exactly as distributed — one file per leaf, byte-for-byte, no rendering, filtering, or normalization. This distribution publishes only the cryptographic manifest (per-file digests + Merkle root) and its signature, not the book texts themselves.
- **License of this distribution:** **Apache-2.0**, matching the source compilation.
