//! databricks-dolly-15k row → natural-text rendering for the Tier-1 seal
//! generator (`examples/seal-dolly.rs`).
//!
//! This file lives under `examples/dolly/` (a *subdirectory*, so Cargo does NOT
//! treat it as an example binary target). It is included two ways, mirroring the
//! WikiText arrangement:
//!   - `examples/seal-dolly.rs` via `#[path = "dolly/render.rs"] mod render;`
//!   - `tests/dolly_render.rs` via `#[path = "../examples/dolly/render.rs"] mod render;`
//!     so its `#[cfg(test)]` unit tests run under `cargo test --workspace`
//!     (examples are not test-gated by default).
//!
//! **One dolly row = one sealed leaf, rendered to natural text** (founder
//! decision, 2026-06-06). A row has four columns — `instruction`, `context`
//! (often empty), `response`, `category`. The sealed bytes are `instruction`,
//! then the `context` block **only when non-empty**, then `response`, each
//! separated by a single blank line and ending in exactly one newline. The bare
//! `category` label is a metadata tag, not training text, so it is dropped.
//! Pure and deterministic. Unlike WikiText-103-raw, dolly is already natural
//! English, so there is no detokenization step — the PROTECTED
//! `attestrum-fingerprint` normalization (CLAUDE.md §4) is untouched.

/// One databricks-dolly-15k row, carrying just the three text columns that get
/// sealed. `category` is intentionally absent — it is not part of the rendered
/// leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DollyRow {
    pub instruction: String,
    pub context: String,
    pub response: String,
}

/// Render one row to the natural-text bytes that become a single sealed leaf.
///
/// Fields are trimmed of surrounding whitespace; empty fields are skipped (so an
/// empty `context` produces no blank-line gap, and a degenerate empty
/// `instruction`/`response` cannot introduce a leading/trailing blank line). The
/// non-empty parts are joined with a single blank line and the result ends in
/// exactly one `\n`. Deterministic: same row in → same bytes out.
pub fn render(row: &DollyRow) -> String {
    let parts: Vec<&str> = [
        row.instruction.trim(),
        row.context.trim(),
        row.response.trim(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();
    let mut out = parts.join("\n\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(instruction: &str, context: &str, response: &str) -> DollyRow {
        DollyRow {
            instruction: instruction.to_string(),
            context: context.to_string(),
            response: response.to_string(),
        }
    }

    #[test]
    fn renders_with_context_block() {
        let r = row(
            "When did Virgin Australia start operating?",
            "Virgin Australia commenced services on 31 August 2000.",
            "Virgin Australia started operating on 31 August 2000.",
        );
        assert_eq!(
            render(&r),
            "When did Virgin Australia start operating?\n\n\
             Virgin Australia commenced services on 31 August 2000.\n\n\
             Virgin Australia started operating on 31 August 2000.\n"
        );
    }

    #[test]
    fn empty_context_produces_no_gap() {
        let r = row("Why is the sky blue?", "", "Rayleigh scattering.");
        // No blank-line gap where the empty context would have been.
        assert_eq!(render(&r), "Why is the sky blue?\n\nRayleigh scattering.\n");
    }

    #[test]
    fn whitespace_only_context_is_treated_as_empty() {
        let r = row("Q one two three", "   \n  ", "A one two three");
        assert_eq!(render(&r), "Q one two three\n\nA one two three\n");
    }

    #[test]
    fn fields_are_trimmed_and_single_trailing_newline() {
        let r = row("  padded instruction  ", "", "  padded response\n\n");
        assert_eq!(render(&r), "padded instruction\n\npadded response\n");
    }

    #[test]
    fn render_is_deterministic() {
        let r = row("instruction here", "some context", "the response");
        assert_eq!(render(&r), render(&r));
    }
}
