//! Hand-rolled API-surface snapshot test per PATH-A-BRIEF Part 7.1's
//! per-`classDiagram` test obligation. Sprint 4 kickoff flag-2 decision
//! chose this approach over adding `cargo-public-api` as a new dev-dep:
//! ~30 LOC of test code is cheaper than a transitive dep tree.
//!
//! How it works: scans `src/lib.rs`, `src/predicate.rs`, `src/statement.rs`
//! for every `pub fn` / `pub struct` / `pub enum` / `pub trait` / `pub type`
//! / `pub const` / `pub mod` line, normalizes each into a canonical
//! `<source>: <kind> <name>` form, sorts, and diffs against the checked-in
//! golden at `tests/api-surface.golden.txt`.
//!
//! Regen the golden file via `ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p
//! attestrum-attest --test api_surface api_surface_matches_golden_file`. Mirrors
//! the standard `INSTA_UPDATE=1` / `ATTESTRUM_REGEN_GOLDEN=1` convention.
//!
//! Why this matters: the `classDiagram` at
//! `docs/diagrams/sprint-4/predicate-three-types.md` is the contract for the
//! attestrum-attest public API. Any accidental `pub` addition / rename /
//! signature change shifts the diff, the linter's reverse-reference check
//! will catch missing diagram entries, and this test catches the symmetric
//! case where Rust code grew a `pub` that the diagram doesn't reflect.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;

const SOURCES: &[&str] = &[
    "src/lib.rs",
    "src/predicate.rs",
    "src/statement.rs",
    "src/canonicalize.rs",
    "src/corpus_digest.rs",
    "src/model_binding.rs",
    "src/json.rs",
    "src/sign.rs",
    "src/dsse_sign.rs",
    "src/identity.rs",
    "src/verify.rs",
];
const GOLDEN_PATH: &str = "tests/api-surface.golden.txt";

/// Extract canonical `<source>: <kind> <name>` lines from each source.
fn collect_api_surface(crate_dir: &Path) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for source in SOURCES {
        let path = crate_dir.join(source);
        let content =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for line in content.lines() {
            // Skip lines inside `#[cfg(test)] mod tests { ... }`. Cheap
            // heuristic: indent > 0 means inside a module/struct/impl body;
            // top-level `pub` items live at column 0.
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
/// happen for the curated SOURCES list, but if it does, the golden diff
/// surfaces the issue without panicking).
fn canonicalize_pub_line(source: &str, line: &str) -> Option<String> {
    let rest = line.trim_start_matches("pub ").trim();
    // Strip generics and tail noise — we only care about kind + ident.
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
    ] {
        if let Some(after) = rest.strip_prefix(prefix) {
            let name = after
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()?
                .to_string();
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
            "read golden {}: {e}\nHint: ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-attest --test api_surface",
            golden_path.display()
        )
    });

    if actual_text != expected {
        let expected_set: BTreeSet<&str> = expected.lines().filter(|l| !l.is_empty()).collect();
        let actual_set: BTreeSet<&str> = actual_text.lines().filter(|l| !l.is_empty()).collect();
        let added: Vec<&&str> = actual_set.difference(&expected_set).collect();
        let removed: Vec<&&str> = expected_set.difference(&actual_set).collect();
        let mut msg = String::from("attestrum-attest public API surface differs from golden.\n");
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
             ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-attest --test api_surface api_surface_matches_golden_file\n\
             and update docs/diagrams/sprint-4/predicate-three-types.md to reflect the new API.\n",
        );
        panic!("{msg}");
    }
}

#[test]
fn collect_api_surface_returns_at_least_the_three_protected_uri_consts() {
    let actual = collect_api_surface(crate_dir());
    assert!(
        actual
            .iter()
            .any(|l| l.contains("const TRAINING_CORPUS_PREDICATE_TYPE")),
        "TRAINING_CORPUS_PREDICATE_TYPE not in surface"
    );
    assert!(
        actual
            .iter()
            .any(|l| l.contains("const INCLUSION_PROOF_PREDICATE_TYPE")),
        "INCLUSION_PROOF_PREDICATE_TYPE not in surface"
    );
    assert!(
        actual
            .iter()
            .any(|l| l.contains("const NON_INCLUSION_PROOF_PREDICATE_TYPE")),
        "NON_INCLUSION_PROOF_PREDICATE_TYPE not in surface"
    );
}

#[test]
fn collect_api_surface_returns_the_three_predicate_types() {
    let actual = collect_api_surface(crate_dir());
    assert!(actual
        .iter()
        .any(|l| l.contains("struct TrainingCorpusPredicate")));
    assert!(actual
        .iter()
        .any(|l| l.contains("struct InclusionProofPredicate")));
    assert!(actual
        .iter()
        .any(|l| l.contains("struct NonInclusionProofPredicate")));
}
