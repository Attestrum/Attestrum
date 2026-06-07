//! Pulls the Tier-1 dolly seal generator's row-rendering module (which lives
//! under `examples/dolly/`, outside the default test set) into
//! `cargo test --workspace` so the six commit gates (CLAUDE.md §7) cover it.
//!
//! The exhaustive cases live in the module's own `#[cfg(test)]` block — included
//! here via `#[path]`, they are collected and run as tests of this crate. The
//! check below is an integration-level smoke test over the public surface.

#[path = "../examples/dolly/render.rs"]
mod render;

use render::{render, DollyRow};

#[test]
fn public_surface_smoke() {
    // Empty context: no blank-line gap, single trailing newline, category absent.
    let r = DollyRow {
        instruction: "List two fruits.".to_string(),
        context: String::new(),
        response: "Apple and banana.".to_string(),
    };
    assert_eq!(render(&r), "List two fruits.\n\nApple and banana.\n");
}
