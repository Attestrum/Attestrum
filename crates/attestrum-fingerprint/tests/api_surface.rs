//! Hand-rolled API-surface snapshot test per PATH-A-BRIEF Part 7.1's
//! per-`classDiagram` test obligation, mirroring the proven
//! `crates/attestrum-attest/tests/api_surface.rs` precedent (Sprint 4
//! kickoff flag-2 decision: ~30 LOC of test code over a `cargo-public-api`
//! transitive dep tree).
//!
//! How it works: scans `src/lib.rs` for every `pub fn` / `pub struct` /
//! `pub enum` / `pub trait` / `pub type` / `pub const` / `pub mod` line,
//! normalizes each into a canonical `<source>: <kind> <name>` form, sorts
//! into a `BTreeSet`, and diffs against the checked-in golden at
//! `tests/api-surface.golden.txt`.
//!
//! Regen the golden file via `ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p
//! attestrum-fingerprint --test api_surface api_surface_matches_golden_file`.
//! Mirrors the `INSTA_UPDATE=1` / `ATTESTRUM_REGEN_GOLDEN=1` /
//! `ATTESTRUM_REGEN_SCHEMAS=1` conventions established by prior commits.
//!
//! Why this matters: `docs/diagrams/sprint-5/fingerprint-pipeline.md` is
//! the contract for the attestrum-fingerprint public API. Any accidental
//! `pub` addition / rename / signature change shifts the diff, the
//! linter's reverse-reference check catches missing diagram entries, and
//! this test catches the symmetric case where Rust code grew a `pub` that
//! the diagram doesn't reflect. Lands at Sprint 5 S5-D1 E5 — the API
//! freeze gate that lets `attestrum-prove` (Sprint 5 E9) build against
//! a stable v0.1 surface.
//!
//! Note: `src/text/{mod,minhash,simhash}.rs` are deliberately scanned
//! ONLY for their (absent) `pub` items — those modules are `pub(crate)`
//! implementation detail per the existing crate-level docs. The current
//! public surface lives entirely in `src/lib.rs`; scanning lib only is
//! the right scope.

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
        for line in content.lines() {
            // Top-level `pub` items live at column 0; nested items inside
            // module / struct / impl bodies are indented and skipped.
            if !line.starts_with("pub ") {
                continue;
            }
            if let Some(entry) = canonicalize_pub_line(source, line) {
                out.insert(entry);
            }
        }
    }
    out.into_iter().collect()
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
            "read golden {}: {e}\nHint: ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-fingerprint --test api_surface",
            golden_path.display()
        )
    });

    if actual_text != expected {
        let expected_set: BTreeSet<&str> = expected.lines().filter(|l| !l.is_empty()).collect();
        let actual_set: BTreeSet<&str> = actual_text.lines().filter(|l| !l.is_empty()).collect();
        let added: Vec<&&str> = actual_set.difference(&expected_set).collect();
        let removed: Vec<&&str> = expected_set.difference(&actual_set).collect();
        let mut msg =
            String::from("attestrum-fingerprint public API surface differs from golden.\n");
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
             ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-fingerprint --test api_surface api_surface_matches_golden_file\n\
             and update docs/diagrams/sprint-5/fingerprint-pipeline.md to reflect the new API.\n",
        );
        panic!("{msg}");
    }
}

#[test]
fn collect_api_surface_returns_the_protected_schema_const() {
    let actual = collect_api_surface(crate_dir());
    assert!(
        actual
            .iter()
            .any(|l| l.contains("const FINGERPRINT_SCHEMA")),
        "FINGERPRINT_SCHEMA not in surface — the v0.1 schema URI is PROTECTED per CLAUDE.md §4"
    );
}

#[test]
fn collect_api_surface_returns_the_four_bundle_types() {
    let actual = collect_api_surface(crate_dir());
    for type_name in [
        "struct FingerprintBundle",
        "struct TextFingerprint",
        "struct ImageFingerprint",
        "struct IsccComposition",
    ] {
        assert!(
            actual.iter().any(|l| l.contains(type_name)),
            "{type_name} not in surface — required for FingerprintBundle JSON Schema derivation"
        );
    }
}

#[test]
fn collect_api_surface_returns_the_two_fingerprint_entry_points() {
    let actual = collect_api_surface(crate_dir());
    for fn_name in ["fn fingerprint_text", "fn fingerprint_image"] {
        assert!(
            actual.iter().any(|l| l.contains(fn_name)),
            "{fn_name} not in surface — required by attestrum-prove at Sprint 5 E9"
        );
    }
}
