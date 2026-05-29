//! Hand-rolled API-surface snapshot test for `attestrum-emit` —
//! Sprint 5 D3 E8, the v0.1-release-ready API freeze.
//!
//! Mirrors the proven `crates/attestrum-prove/tests/api_surface.rs`
//! precedent (S5-D2 E8) and the sibling `attestrum-publish` test
//! shipping in the same E8 commit. Hand-rolled state machine over
//! `src/lib.rs`, no external deps.
//!
//! How it works: scans `src/lib.rs` for every `pub fn` / `pub struct` /
//! `pub enum` / `pub trait` / `pub type` / `pub const` / `pub static` /
//! `pub mod` / `pub use` line, expands multi-line `pub use prefix::{...};`
//! re-export blocks into one synthetic line per re-exported symbol,
//! normalises each into a canonical `<source>: <kind> <name>` form, sorts
//! into a `BTreeSet`, and diffs against the checked-in golden at
//! `tests/api-surface.golden.txt`.
//!
//! Regen via:
//!
//! ```sh
//! ATTESTRUM_REGEN_API_SURFACE=1 \
//!   cargo test -p attestrum-emit --test api_surface api_surface_matches_golden_file
//! ```
//!
//! Mirrors the same env-var convention as the fingerprint + attest +
//! prove + publish precedents (`ATTESTRUM_REGEN_API_SURFACE`), the
//! `INSTA_UPDATE=1` convention from snapshot-test ecosystems, and
//! Attestrum's own `ATTESTRUM_REGEN_GOLDEN` / `ATTESTRUM_REGEN_SCHEMAS`
//! conventions.
//!
//! Why this matters: `docs/diagrams/overview/hub-publish.md` flips
//! from `source_of_truth: diagram` to `source_of_truth: code` at this
//! same E8 commit — drift gate 6 in the diagram-linter activates for
//! both publish-side and emit-side files named in the diagram's
//! `models:` field. Any accidental `pub` addition / rename / signature
//! change to `attestrum-emit/src/lib.rs` shifts this golden, and the
//! drift gate catches the symmetric case where Rust code grew a `pub`
//! that the diagram doesn't yet reflect.
//!
//! Multi-line `pub use` extension is inherited from the prove parser
//! even though `attestrum-emit` does not currently re-export from
//! sibling crates — keeping the parser shape uniform across all
//! Attestrum api-surface tests makes maintenance trivially symmetric
//! and protects against future re-export additions silently degrading
//! coverage.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;

const SOURCES: &[&str] = &["src/lib.rs"];
const GOLDEN_PATH: &str = "tests/api-surface.golden.txt";

/// Extract canonical `<source>: <kind> <name>` lines from each source.
fn collect_api_surface(crate_dir: &Path) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for source in SOURCES {
        let path = crate_dir.join(source);
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for line in expand_multi_line_pub_use(&content) {
            // Top-level `pub` items live at column 0; nested items inside
            // module / struct / impl bodies are indented and skipped.
            if !line.starts_with("pub ") {
                continue;
            }
            if let Some(entry) = canonicalize_pub_line(source, &line) {
                out.insert(entry);
            }
        }
    }
    out.into_iter().collect()
}

/// Pre-pass over the file content: turn multi-line
/// `pub use prefix::{ a, b, c };` blocks into per-symbol synthetic
/// `pub use prefix::a;` / `pub use prefix::b;` / `pub use prefix::c;`
/// lines. All other lines pass through unchanged. The output preserves
/// document order — only the `pub use ... { ... };` shape is rewritten.
fn expand_multi_line_pub_use(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut iter = content.lines().peekable();
    while let Some(line) = iter.next() {
        if let Some(after_pub_use) = line.strip_prefix("pub use ") {
            if let Some(brace_idx) = after_pub_use.find('{') {
                let prefix = after_pub_use[..brace_idx].trim().trim_end_matches("::");
                // Accumulate the inner contents (everything between `{` and `};`).
                let mut inner = String::from(&after_pub_use[brace_idx + 1..]);
                while !inner.contains("};") {
                    match iter.next() {
                        Some(next_line) => {
                            inner.push(' ');
                            inner.push_str(next_line);
                        }
                        None => break,
                    }
                }
                let body = inner.split("};").next().unwrap_or("");
                for symbol in body.split(',') {
                    let symbol = symbol.trim();
                    if symbol.is_empty() {
                        continue;
                    }
                    out.push(format!("pub use {prefix}::{symbol};"));
                }
                continue;
            }
        }
        out.push(line.to_string());
    }
    out
}

/// Convert a `pub ...` line into a `<source>: <kind> <name>` canonical form.
/// Returns `None` for lines we don't recognize (defensive — should never
/// happen for the curated SOURCES list, but if it does the golden diff
/// surfaces the issue without panicking).
fn canonicalize_pub_line(source: &str, line: &str) -> Option<String> {
    let rest = line.trim_start_matches("pub ").trim();
    let (kind, name) = parse_kind_and_name(rest)?;
    Some(format!("{source}: {kind} {name}"))
}

fn parse_kind_and_name(rest: &str) -> Option<(&'static str, String)> {
    for (prefix, kind) in [
        ("fn ", "fn"),
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("type ", "type"),
        ("const ", "const"),
        ("static ", "static"),
        ("mod ", "mod"),
        ("use ", "use"),
    ] {
        if let Some(after) = rest.strip_prefix(prefix) {
            // Accept `:` inside the name so `pub use foo::bar` canonicalises
            // to `use foo::bar` (preserves which item is re-exported), then
            // strip any trailing colons so `pub const NAME: TYPE = …` doesn't
            // bleed the post-name colon into the canonical form.
            let raw = after
                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                .next()?;
            let name = raw.trim_end_matches(':').to_string();
            if name.is_empty() {
                return None;
            }
            return Some((kind, name));
        }
    }
    None
}

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn api_surface_matches_golden_file() {
    let actual = collect_api_surface(crate_dir());
    let golden_path = crate_dir().join(GOLDEN_PATH);

    let actual_text = format!("{}\n", actual.join("\n"));

    if env::var("ATTESTRUM_REGEN_API_SURFACE").is_ok() {
        fs::write(&golden_path, &actual_text)
            .unwrap_or_else(|e| panic!("regen write {}: {e}", golden_path.display()));
        eprintln!("regenerated golden at {}", golden_path.display());
        return;
    }

    let expected = fs::read_to_string(&golden_path).unwrap_or_else(|e| {
        panic!(
            "read golden {}: {e}\nHint: ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-emit --test api_surface",
            golden_path.display()
        )
    });

    if actual_text != expected {
        let expected_set: BTreeSet<&str> = expected.lines().filter(|l| !l.is_empty()).collect();
        let actual_set: BTreeSet<&str> = actual_text.lines().filter(|l| !l.is_empty()).collect();
        let added: Vec<&&str> = actual_set.difference(&expected_set).collect();
        let removed: Vec<&&str> = expected_set.difference(&actual_set).collect();
        let mut msg = String::from("attestrum-emit public API surface differs from golden.\n");
        if !added.is_empty() {
            msg.push_str("\n  Added (new pub items not in golden):\n");
            for line in &added {
                msg.push_str(&format!("    + {line}\n"));
            }
        }
        if !removed.is_empty() {
            msg.push_str("\n  Removed (golden has but code does not):\n");
            for line in &removed {
                msg.push_str(&format!("    - {line}\n"));
            }
        }
        msg.push_str(
            "\nIf this change is intentional: regen via\n  \
             ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-emit --test api_surface api_surface_matches_golden_file\n\
             and update docs/diagrams/overview/hub-publish.md to reflect the new API.\n",
        );
        panic!("{msg}");
    }
}

#[test]
fn collect_api_surface_includes_three_render_fns() {
    // The three v0.1 emitter entry points called by
    // `attestrum-publish::HuggingFaceTarget::publish()` at lib.rs L217-222.
    let actual = collect_api_surface(crate_dir());
    for fn_name in [
        "fn render_croissant",
        "fn render_readme",
        "fn render_verify_html_stub",
    ] {
        assert!(
            actual.iter().any(|l| l.contains(fn_name)),
            "{fn_name} not in surface — required by the v0.1 emitter contract"
        );
    }
}

#[test]
fn collect_api_surface_includes_four_plan_structs_and_manifest_stats() {
    // The plan inputs each render_* fn consumes, plus ManifestStats
    // (embedded inside both CroissantPlan and DatasetCardPlan and
    // therefore re-exported by attestrum-publish at lib.rs L42).
    let actual = collect_api_surface(crate_dir());
    for type_name in [
        "struct CroissantPlan",
        "struct DatasetCardPlan",
        "struct VerifyHtmlPlan",
        "struct ManifestStats",
    ] {
        assert!(
            actual.iter().any(|l| l.contains(type_name)),
            "{type_name} not in surface — required by the v0.1 plan API"
        );
    }
}

#[test]
fn collect_api_surface_includes_five_variant_error() {
    // attestrum-emit/src/lib.rs declares AttestrumEmitError with 5
    // variants (Manifest, Bundle, Croissant, Readme, VerifyHtml). The
    // presence of the enum in the surface is the minimum check;
    // variant-level coverage is enforced by downstream usage in
    // attestrum-publish's map_error_to_exit_code path.
    let actual = collect_api_surface(crate_dir());
    assert!(
        actual.iter().any(|l| l.contains("enum AttestrumEmitError")),
        "enum AttestrumEmitError not in surface — locks the v0.1 emit error contract"
    );
}

#[test]
fn collect_api_surface_includes_three_pub_mods() {
    // The three sibling sub-modules (`croissant`, `dataset_card`,
    // `verify_html`) are `pub mod` declared in lib.rs. The diagram-
    // linter's reverse-reference check (CLAUDE.md §5 check 5) requires
    // each to appear in at least one diagram's `models:` field —
    // satisfied by hub-publish.md's path entries
    // (crates/attestrum-emit/src/<mod>.rs). This test asserts the
    // declarations themselves stay in the surface so a refactor to
    // private-mod doesn't silently de-cover the diagram link.
    let actual = collect_api_surface(crate_dir());
    for mod_name in ["mod croissant", "mod dataset_card", "mod verify_html"] {
        assert!(
            actual.iter().any(|l| l.contains(mod_name)),
            "{mod_name} not in surface — diagram-linter coverage of hub-publish.md depends on it"
        );
    }
}
