//! WikiText-103 passage segmentation + moses detokenization for the Lookback
//! seal generator (`examples/seal-wikitext.rs`).
//!
//! This file lives under `examples/wikitext/` (a *subdirectory*, so Cargo does
//! NOT treat it as an example binary target). It is included two ways:
//!   - `examples/seal-wikitext.rs` via `#[path = "wikitext/segment.rs"] mod segment;`
//!   - `tests/wikitext_segment.rs` via
//!     `#[path = "../examples/wikitext/segment.rs"] mod segment;`
//!     so its `#[cfg(test)]` unit tests run under `cargo test --workspace`
//!     (examples are not test-gated by default; this mirrors the established
//!     `sprint-3-corpus` / `tests/cross_platform_inputs.rs` arrangement).
//!
//! **Detokenization lives HERE, in segmentation — never in the PROTECTED
//! `attestrum-fingerprint` normalization (CLAUDE.md §4).** WikiText-103-raw still
//! ships moses-tokenized (` @-@ `, spaced punctuation); detokenizing each passage
//! back to natural English is what lets a pasted paragraph match (Phase-0:
//! ~0.32 -> ~1.00 Jaccard). The CAS therefore stores the detokenized bytes.

/// Minimum word count for a body line to become a sealed passage. Lines below
/// this (stray fragments, one-line stubs) are dropped so the index isn't full of
/// un-matchable scraps.
pub const MIN_PASSAGE_WORDS: usize = 5;

/// A segmented, detokenized passage ready to become one corpus leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    /// Source backref of the form `wikipedia://<slug>#p<N>` (N is 1-based within
    /// the article).
    pub source_uri: String,
    /// Detokenized natural-English passage text — exactly the bytes that get
    /// sealed into the CAS / manifest.
    pub text: String,
}

/// Detokenize one WikiText-103-raw line into natural English.
///
/// Reverses the moses tokenization wikitext-103-raw still carries: the `@-@` /
/// `@,@` / `@.@` joiners, and the spaces moses inserts around punctuation,
/// brackets, and split contractions. Pure and deterministic.
pub fn detokenize(line: &str) -> String {
    // Step 1: WikiText-103 joiners (always space-padded in the corpus).
    let pre = line
        .replace(" @-@ ", "-")
        .replace(" @,@ ", ",")
        .replace(" @.@ ", ".");

    // Step 2: re-attach punctuation/brackets/contractions over the moses
    // single-space token stream.
    let mut out = String::with_capacity(pre.len());
    let mut prev_was_open = false;
    for (i, tok) in pre.split_whitespace().enumerate() {
        let attach_left = i == 0 || prev_was_open || attaches_left(tok);
        if !attach_left {
            out.push(' ');
        }
        out.push_str(tok);
        prev_was_open = is_open(tok);
    }
    out
}

/// Tokens that hug the *preceding* token (no space before them): sentence /
/// clause punctuation, closing brackets, closing quotes, and split
/// contractions / possessives (`'s`, `n't`, `'re`, ...).
fn attaches_left(tok: &str) -> bool {
    matches!(
        tok,
        "," | "." | ";" | ":" | "!" | "?" | "%" | ")" | "]" | "}" | "''" | "’" | "n't" | "n’t"
    ) || (tok.starts_with('\'') && tok.len() > 1)
        || (tok.starts_with('’') && tok.chars().count() > 1)
}

/// Tokens after which the *following* token hugs (no space after them): opening
/// brackets and opening quotes.
fn is_open(tok: &str) -> bool {
    matches!(tok, "(" | "[" | "{" | "``" | "‘")
}

/// Classify a heading line.
///
/// Returns `Some(true)` for a level-1 article title (` = Title = `),
/// `Some(false)` for a level-2+ section heading (` = = Section = = `), and
/// `None` for an ordinary body line. WikiText-103 separates the `=` markers with
/// spaces, so a section's inner content is itself a smaller heading.
fn classify_heading(trimmed: &str) -> Option<bool> {
    if !trimmed.starts_with('=') || !trimmed.ends_with('=') {
        return None;
    }
    let inner = trimmed
        .strip_prefix('=')
        .and_then(|s| s.strip_suffix('='))
        .map(str::trim)
        .unwrap_or("");
    if inner.is_empty() {
        return None;
    }
    Some(!(inner.starts_with('=') && inner.ends_with('=')))
}

/// Slugify a (detokenized) article title into the `<slug>` of the source
/// backref. Wikipedia-style: collapse whitespace runs to single underscores.
fn slugify(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join("_")
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Segment a WikiText-103 `text` document into detokenized passages.
///
/// Each non-empty, non-heading line becomes one passage (after detokenization
/// and the [`MIN_PASSAGE_WORDS`] floor); a level-1 ` = Title = ` line sets the
/// article slug and resets the per-article passage counter; section headings are
/// skipped. Deterministic: input order in -> passage order out.
pub fn segment(text: &str) -> Vec<Passage> {
    let mut out = Vec::new();
    let mut slug = String::from("_unknown");
    let mut passage_idx = 0usize;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        match classify_heading(line) {
            Some(true) => {
                let title = detokenize(line.trim_matches('=').trim());
                slug = slugify(&title);
                passage_idx = 0;
                continue;
            }
            Some(false) => continue, // section heading
            None => {}
        }
        let detok = detokenize(line);
        if word_count(&detok) < MIN_PASSAGE_WORDS {
            continue;
        }
        passage_idx += 1;
        out.push(Passage {
            source_uri: format!("wikipedia://{slug}#p{passage_idx}"),
            text: detok,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jaccard over the word-token sets of two strings (1.0 == identical sets).
    fn jaccard(a: &str, b: &str) -> f64 {
        use std::collections::BTreeSet;
        let sa: BTreeSet<&str> = a.split_whitespace().collect();
        let sb: BTreeSet<&str> = b.split_whitespace().collect();
        if sa.is_empty() && sb.is_empty() {
            return 1.0;
        }
        let inter = sa.intersection(&sb).count() as f64;
        let union = sa.union(&sb).count() as f64;
        inter / union
    }

    #[test]
    fn detok_joiners() {
        assert_eq!(detokenize("the cat @-@ dog hybrid"), "the cat-dog hybrid");
        assert_eq!(detokenize("1 @,@ 000 deaths"), "1,000 deaths");
        assert_eq!(detokenize("pi is 3 @.@ 14 today"), "pi is 3.14 today");
    }

    #[test]
    fn detok_punctuation_and_brackets() {
        assert_eq!(detokenize("Hello , world ."), "Hello, world.");
        assert_eq!(detokenize("wait ; then : go !"), "wait; then: go!");
        assert_eq!(detokenize("a ( b ) c"), "a (b) c");
        assert_eq!(detokenize("see [ 1 ] now"), "see [1] now");
        assert_eq!(detokenize("90 % done"), "90% done");
    }

    #[test]
    fn detok_contractions() {
        assert_eq!(detokenize("do n't stop"), "don't stop");
        assert_eq!(detokenize("it 's fine"), "it's fine");
        assert_eq!(detokenize("they 're here"), "they're here");
        assert_eq!(detokenize("the dog 's bone"), "the dog's bone");
    }

    #[test]
    fn detok_realistic_paragraph_recovers_natural_text() {
        // The natural-English target a visitor would paste.
        let natural = "The 3,000-year-old city wasn't ready, but it's famous (worldwide).";
        // Its moses-tokenized WikiText-103-raw form.
        let tokenized =
            "The 3 @,@ 000 @-@ year @-@ old city was n't ready , but it 's famous ( worldwide ) .";
        let detok = detokenize(tokenized);
        assert_eq!(detok, natural, "detok did not recover the natural text");
        // The whole point of Phase A: raw tokenized text barely overlaps the
        // pasted natural text, detokenized text overlaps strongly.
        assert!(
            jaccard(tokenized, natural) < 0.85,
            "raw tokenized text should NOT match natural text"
        );
        assert!(
            jaccard(&detok, natural) >= 0.85,
            "detokenized text must match natural text (>=0.85 Jaccard)"
        );
    }

    #[test]
    fn heading_classification() {
        assert_eq!(classify_heading("= Valkyria Chronicles III ="), Some(true));
        assert_eq!(classify_heading("= = Gameplay = ="), Some(false));
        assert_eq!(classify_heading("= = = Sub @-@ section = = ="), Some(false));
        assert_eq!(classify_heading("an ordinary line"), None);
        assert_eq!(classify_heading("x = y in the equation"), None);
    }

    #[test]
    fn segment_article_titles_sections_and_floor() {
        let doc = "\
 = Valkyria Chronicles III =

 Senjo no Valkyria 3 is a tactical role @-@ playing video game developed by Sega .

 = = Gameplay = =

 The game is a tactical role @-@ playing game where players control a squad .

 too short
";
        let passages = segment(doc);
        assert_eq!(passages.len(), 2, "two body lines clear the word floor");
        assert_eq!(
            passages[0].source_uri,
            "wikipedia://Valkyria_Chronicles_III#p1"
        );
        assert_eq!(
            passages[1].source_uri,
            "wikipedia://Valkyria_Chronicles_III#p2"
        );
        assert!(passages[0].text.contains("role-playing"));
        // "too short " is below MIN_PASSAGE_WORDS and must be dropped.
        assert!(passages.iter().all(|p| !p.text.contains("too short")));
    }

    #[test]
    fn segment_is_deterministic() {
        let doc = " = A = \n\n the quick brown fox jumps over the lazy dog .\n";
        assert_eq!(segment(doc), segment(doc));
    }
}
