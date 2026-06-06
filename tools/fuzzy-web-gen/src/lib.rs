//! Generator for the bounded fuzzy-match corpus artifact (`fuzzy-index.json`).
//!
//! The attestrum.com near-match demo loads this JSON in the browser, computes a
//! pasted query's MinHash with the C2 wasm kernel, and brute-forces exact
//! Jaccard against the corpus signatures here — at this size (5 leaves) the
//! brute force is the exhaustive recall oracle, byte-identical in result to the
//! production LSH candidate path, with no banding machinery.
//!
//! Signatures are computed with [`attestrum_text_minhash`] — the SAME PROTECTED
//! kernel the wasm compiles from and that `attestrum index build` /
//! `attestrum prove` use. So the corpus signatures shipped here are byte-
//! identical to what the browser's wasm recomputes (C2's cross-check gate proves
//! wasm == kernel) and to what the CLI would match against.

use attestrum_text_minhash::{minhash, normalize_text};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// MinHash width (PROTECTED kernel parameter, mirrored for the browser).
pub const SIG_WIDTH: u32 = 128;
/// LSH bands (informational for the browser; the bounded demo brute-forces).
pub const BANDS: u32 = 32;
/// LSH rows per band (BANDS × ROWS == SIG_WIDTH).
pub const ROWS: u32 = 4;
/// Inclusion threshold in parts-per-million (0.85 Jaccard), mirroring
/// `attestrum-prove`'s `FUZZY_THRESHOLD_MINHASH_JACCARD_PPM`.
pub const JACCARD_THRESHOLD_PPM: u32 = 850_000;
/// Word-shingle size (PROTECTED kernel parameter).
pub const NGRAM: u32 = 5;

/// One row of `display.json`: provenance + presentation metadata for a passage.
/// `file` is resolved relative to the same directory as `display.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct DisplayEntry {
    pub file: String,
    pub title: String,
    pub url: String,
    #[serde(rename = "passageId")]
    pub passage_id: String,
}

/// Mirrored kernel/threshold parameters, emitted so the browser glue stays in
/// sync with the Rust constants without hardcoding them in JS.
#[derive(Debug, Serialize)]
pub struct Params {
    #[serde(rename = "sigWidth")]
    pub sig_width: u32,
    pub bands: u32,
    pub rows: u32,
    #[serde(rename = "jaccardThresholdPpm")]
    pub jaccard_threshold_ppm: u32,
    pub ngram: u32,
}

/// One corpus leaf: display metadata + its 128-permutation MinHash signature as
/// lowercase 16-hex little-endian `u64` strings (JSON numbers can't hold `u64`).
#[derive(Debug, Serialize)]
pub struct Leaf {
    pub row: u32,
    pub title: String,
    pub url: String,
    #[serde(rename = "passageId")]
    pub passage_id: String,
    pub snippet: String,
    pub sig: Vec<String>,
}

/// The full browser artifact.
#[derive(Debug, Serialize)]
pub struct FuzzyWebIndex {
    pub version: u32,
    pub kind: String,
    pub corpus: String,
    pub params: Params,
    pub leaves: Vec<Leaf>,
}

/// Compute the MinHash of one passage as 16-hex LE `u64` strings, via the
/// PROTECTED kernel (identical to the wasm and to `attestrum index build`).
pub fn signature_hex(text: &str) -> Vec<String> {
    minhash::compute(&normalize_text(text))
        .iter()
        .map(|v| format!("{v:016x}"))
        .collect()
}

/// Build the in-memory artifact from `(display, passage_text)` pairs in row
/// order. `corpus` labels the provenance (e.g. `"wikitext-103-sealed"`).
pub fn build_fuzzy_index(entries: &[(DisplayEntry, String)], corpus: &str) -> FuzzyWebIndex {
    let leaves = entries
        .iter()
        .enumerate()
        .map(|(i, (d, text))| Leaf {
            row: i as u32,
            title: d.title.clone(),
            url: d.url.clone(),
            passage_id: d.passage_id.clone(),
            snippet: text.clone(),
            sig: signature_hex(text),
        })
        .collect();

    FuzzyWebIndex {
        version: 1,
        kind: "minhash".to_string(),
        corpus: corpus.to_string(),
        params: Params {
            sig_width: SIG_WIDTH,
            bands: BANDS,
            rows: ROWS,
            jaccard_threshold_ppm: JACCARD_THRESHOLD_PPM,
            ngram: NGRAM,
        },
        leaves,
    }
}

/// Deterministic JSON serialization (pretty, struct field order is fixed, no
/// maps) with a trailing newline — byte-identical on every regeneration (§7).
pub fn to_json(index: &FuzzyWebIndex) -> String {
    let mut s = serde_json::to_string_pretty(index).expect("FuzzyWebIndex serializes");
    s.push('\n');
    s
}

/// Load `display.json` from `fixtures_dir` and read each referenced passage file
/// (verbatim — the byte-exact sealed leaf) from the same directory, in row order.
pub fn load_entries(fixtures_dir: &Path) -> std::io::Result<Vec<(DisplayEntry, String)>> {
    let display_raw = std::fs::read_to_string(fixtures_dir.join("display.json"))?;
    let display: Vec<DisplayEntry> = serde_json::from_str(&display_raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::with_capacity(display.len());
    for d in display {
        let text = std::fs::read_to_string(fixtures_dir.join(&d.file))?;
        out.push((d, text));
    }
    Ok(out)
}
