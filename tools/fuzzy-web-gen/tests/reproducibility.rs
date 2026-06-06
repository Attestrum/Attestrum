//! Reproducibility gate: the committed `fuzzy-index.json` must equal a fresh
//! regeneration from the showcase fixtures, and each leaf signature must equal
//! the PROTECTED kernel's output for that passage. Runs in `cargo test
//! --workspace`. If this fails after an intentional passage/display change,
//! regenerate the golden:
//!
//! ```text
//! cargo run -p fuzzy-web-gen -- tests/fixtures/fuzzy-web/fuzzy-index.json
//! ```

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // <repo>/tools/fuzzy-web-gen → <repo>
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn golden_matches_fresh_regeneration() {
    let root = repo_root();
    let fixtures = root.join("tests/fixtures/showcase-passages");
    let entries = fuzzy_web_gen::load_entries(&fixtures).expect("load showcase fixtures");
    let index = fuzzy_web_gen::build_fuzzy_index(&entries, "wikitext-103-sealed");
    let regenerated = fuzzy_web_gen::to_json(&index);

    let golden_path = root.join("tests/fixtures/fuzzy-web/fuzzy-index.json");
    let golden = std::fs::read_to_string(&golden_path).expect("read committed golden");

    assert_eq!(
        regenerated, golden,
        "fuzzy-index.json drift — regenerate with: \
         cargo run -p fuzzy-web-gen -- tests/fixtures/fuzzy-web/fuzzy-index.json"
    );
}

#[test]
fn leaf_signatures_equal_the_kernel() {
    let fixtures = repo_root().join("tests/fixtures/showcase-passages");
    let entries = fuzzy_web_gen::load_entries(&fixtures).expect("load showcase fixtures");
    let index = fuzzy_web_gen::build_fuzzy_index(&entries, "wikitext-103-sealed");

    assert_eq!(
        index.leaves.len(),
        5,
        "the bounded corpus is the 5 showcase passages"
    );

    for ((_, text), leaf) in entries.iter().zip(&index.leaves) {
        let want: Vec<String> =
            attestrum_text_minhash::minhash::compute(&attestrum_text_minhash::normalize_text(text))
                .iter()
                .map(|v| format!("{v:016x}"))
                .collect();
        assert_eq!(
            leaf.sig, want,
            "leaf `{}` sig must equal the kernel",
            leaf.title
        );
        assert_eq!(leaf.sig.len(), 128, "MinHash signature is 128 perms");
        // snippet is the exact byte-source of the signature (enables C4's
        // in-page conformance check: wasm(normalize(snippet)) == sig).
        assert_eq!(&leaf.snippet, text);
    }
}
