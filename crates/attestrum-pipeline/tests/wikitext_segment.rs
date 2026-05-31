//! Pulls the Lookback seal generator's segmentation + detokenization module
//! (which lives under `examples/wikitext/`, outside the default test set) into
//! `cargo test --workspace` so the six commit gates (CLAUDE.md §7) cover it.
//!
//! The exhaustive cases live in the module's own `#[cfg(test)]` block — included
//! here via `#[path]`, they are collected and run as tests of this crate. The
//! checks below are an integration-level smoke test over the public surface.

#[path = "../examples/wikitext/segment.rs"]
mod segment;

use segment::{detokenize, segment as segment_doc, Passage};

#[test]
fn public_surface_smoke() {
    assert_eq!(detokenize("a , b ."), "a, b.");

    // "one ." is below the MIN_PASSAGE_WORDS floor and must be dropped.
    let doc = " = Test Article = \n\n the quick brown fox jumps over the lazy dog .\n one .\n";
    let passages: Vec<Passage> = segment_doc(doc);
    assert_eq!(passages.len(), 1);
    assert_eq!(passages[0].source_uri, "wikipedia://Test_Article#p1");
    assert_eq!(
        passages[0].text,
        "the quick brown fox jumps over the lazy dog."
    );
}
