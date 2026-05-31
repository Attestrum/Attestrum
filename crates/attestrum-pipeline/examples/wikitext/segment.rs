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
/// `@,@` / `@.@` joiners; the em-dash (` -- `) and spaced slash (` / `); and the
/// spaces moses inserts around punctuation, brackets, quotes, currency symbols,
/// and split contractions. Straight double quotes are resolved to opening /
/// closing by alternation within the line (the corpus uses spaced straight `"`,
/// not moses `` `` ``/`''`, for ~a third of passages). Pure and deterministic.
///
/// Best-effort, not byte-perfect: an unbalanced `"` within a single line (a
/// quotation spanning a paragraph break) can mis-resolve, and rare spacing in
/// the source survives. The goal is natural English that matches a pasted
/// passage, not a perfect inverse of moses.
pub fn detokenize(line: &str) -> String {
    // Step 1: joiners + spaced separators that need bidirectional context
    // (cheaper as string replaces than carried through the token loop).
    let pre = line
        .replace(" @-@ ", "-")
        .replace(" @,@ ", ",")
        .replace(" @.@ ", ".")
        .replace(" -- ", "\u{2014}") // em-dash
        .replace(" / ", "/");

    // Step 2: walk the moses single-space token stream. A space is inserted
    // between two tokens unless the current token hugs the previous one
    // (`hug_left`) or the previous token hugs the next (`hug_right`).
    let tokens: Vec<&str> = pre.split_whitespace().collect();
    let mut out = String::with_capacity(pre.len());
    let mut prev_hugs_next = false;
    let mut dquote_open = false;
    for i in 0..tokens.len() {
        let tok = tokens[i];
        let prev_ends_digit = i > 0 && tokens[i - 1].ends_with(|c: char| c.is_ascii_digit());
        let next_starts_digit =
            i + 1 < tokens.len() && tokens[i + 1].starts_with(|c: char| c.is_ascii_digit());

        // (emitted text, hugs_left, hugs_right); updates straight-quote state.
        let (emit, hug_left, hug_right): (&str, bool, bool) = if tok == "\"" {
            if dquote_open {
                dquote_open = false;
                ("\"", true, false) // closing quote hugs the previous token
            } else {
                dquote_open = true;
                ("\"", false, true) // opening quote hugs the next token
            }
        } else if tok == "``" {
            ("\"", false, true) // moses opening double quote
        } else if tok == "''" {
            ("\"", true, false) // moses closing double quote
        } else if matches!(tok, "$" | "\u{00a3}" | "\u{20ac}" | "\u{00a5}") {
            (tok, false, true) // currency symbol hugs the amount that follows
        } else if tok == ":" && prev_ends_digit && next_starts_digit {
            (tok, true, true) // clock time / ratio: 3 : 30 -> 3:30
        } else {
            (tok, attaches_left(tok), is_open(tok))
        };

        if i != 0 && !prev_hugs_next && !hug_left {
            out.push(' ');
        }
        out.push_str(emit);
        prev_hugs_next = hug_right;
    }
    out
}

/// Tokens that hug the *preceding* token (no space before them): sentence /
/// clause punctuation, closing brackets, and split contractions / possessives
/// (`'s`, `n't`, `'re`, ...). Quotes and currency are handled in [`detokenize`].
fn attaches_left(tok: &str) -> bool {
    matches!(
        tok,
        "," | "."
            | ";"
            | ":"
            | "!"
            | "?"
            | "%"
            | ")"
            | "]"
            | "}"
            | "\u{2019}"
            | "n't"
            | "n\u{2019}t"
    ) || (tok.starts_with('\'') && tok.len() > 1)
        || (tok.starts_with('\u{2019}') && tok.chars().count() > 1)
}

/// Tokens after which the *following* token hugs (no space after them): opening
/// brackets and the opening curly quote.
fn is_open(tok: &str) -> bool {
    matches!(tok, "(" | "[" | "{" | "\u{2018}")
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
    // Section heading (level 2+): the inner span is itself `= ... =`.
    if inner.starts_with('=') && inner.ends_with('=') {
        return Some(false);
    }
    // A real Wikipedia article title never contains a semicolon. WikiText-103
    // linearizes some stat-table glossaries as single-`=` lines mid-article
    // (e.g. ` = Goals ; A = `, ` = Wins ; L = `); treat any `;`-bearing
    // single-`=` line as a non-title heading so it neither becomes the article
    // slug nor mis-attributes the passages that follow it.
    if inner.contains(';') {
        return Some(false);
    }
    Some(true)
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
    use std::collections::BTreeMap;
    let mut out = Vec::new();
    let mut slug = String::from("_unknown");
    let mut passage_idx = 0usize;
    // Per-slug occurrence counter so two articles that would otherwise share a
    // slug get distinct backref namespaces (`Foo`, `Foo-2`, ...). Walked in
    // input order, so the assignment is deterministic; the map is only ever
    // point-looked-up, never iterated.
    let mut slug_seen: BTreeMap<String, u32> = BTreeMap::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        match classify_heading(line) {
            Some(true) => {
                let title = detokenize(line.trim_matches('=').trim());
                let base = slugify(&title);
                let n = slug_seen.entry(base.clone()).or_insert(0);
                *n += 1;
                slug = if *n == 1 { base } else { format!("{base}-{n}") };
                passage_idx = 0;
                continue;
            }
            Some(false) => continue, // section heading / mangled glossary line
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

    #[test]
    fn detok_quotes_currency_units_and_emdash() {
        // Straight double quotes (the corpus's dominant quote form) resolve to
        // opening/closing by alternation within the line.
        assert_eq!(
            detokenize("Squad 422 , also known as \" The Nameless \" , are"),
            "Squad 422, also known as \"The Nameless\", are"
        );
        // Currency hugs the amount.
        assert_eq!(detokenize("it cost $ 5 million"), "it cost $5 million");
        // Spaced slash joins; clock time / ratio colon joins between digits.
        assert_eq!(detokenize("5 kg / m at 3 : 30 pm"), "5 kg/m at 3:30 pm");
        // Em-dash.
        assert_eq!(
            detokenize("war -- peace as a theme"),
            "war\u{2014}peace as a theme"
        );
        // moses `` / '' double quotes also map to straight quotes.
        assert_eq!(detokenize("he said `` hi there ''"), "he said \"hi there\"");
    }

    #[test]
    fn heading_rejects_semicolon_glossary_lines() {
        // Real article titles (incl. parenthetical disambiguators) are titles.
        assert_eq!(classify_heading("= Valkyria Chronicles III ="), Some(true));
        assert_eq!(classify_heading("= USS Atlanta ( 1861 ) ="), Some(true));
        // Linearized stat-table glossary lines must NOT be treated as titles.
        assert_eq!(classify_heading("= Goals ; A ="), Some(false));
        assert_eq!(classify_heading("= Wins ; L ="), Some(false));
    }

    #[test]
    fn segment_skips_glossary_and_keeps_attribution() {
        // A `;`-glossary line mid-article must not steal attribution from the
        // real article that precedes it.
        let doc = "\
 = Real Article =

 the quick brown fox jumps over the lazy dog repeatedly today .

 = Goals ; A =

 another full sentence with at least five words here now .
";
        let passages = segment(doc);
        assert_eq!(passages.len(), 2);
        assert_eq!(passages[0].source_uri, "wikipedia://Real_Article#p1");
        // The line after the glossary stays under Real Article (p2), NOT under a
        // spurious "Goals" slug, and the counter does not reset.
        assert_eq!(passages[1].source_uri, "wikipedia://Real_Article#p2");
    }

    #[test]
    fn segment_disambiguates_colliding_slugs() {
        // Two distinct articles with the same title get distinct backrefs.
        let doc = "\
 = Mercury =

 the first article about the planet has enough words to pass the floor .

 = Mercury =

 the second article about the element also clears the word floor easily .
";
        let passages = segment(doc);
        assert_eq!(passages.len(), 2);
        assert_eq!(passages[0].source_uri, "wikipedia://Mercury#p1");
        assert_eq!(passages[1].source_uri, "wikipedia://Mercury-2#p1");
    }
}
