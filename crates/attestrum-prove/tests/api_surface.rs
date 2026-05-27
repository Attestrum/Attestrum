//! Hand-rolled API-surface snapshot test for `attestrum-prove` —
//! Sprint 5 D2 E8, the v0.1-release-ready API freeze.
//!
//! Mirrors the proven `crates/attestrum-fingerprint/tests/api_surface.rs`
//! precedent (S5-D1 E5) and `crates/attestrum-attest/tests/api_surface.rs`
//! (Sprint 4). Hand-rolled state machine over `src/lib.rs`, no external
//! deps. ~30 LOC of declarative test code over a `cargo-public-api`-style
//! transitive dep tree.
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
//!   cargo test -p attestrum-prove --test api_surface api_surface_matches_golden_file
//! ```
//!
//! Mirrors the same env-var convention as the fingerprint + attest
//! precedents (`ATTESTRUM_REGEN_API_SURFACE`), the `INSTA_UPDATE=1`
//! convention from snapshot-test ecosystems, and Attestrum's own
//! `ATTESTRUM_REGEN_GOLDEN` / `ATTESTRUM_REGEN_SCHEMAS` conventions.
//!
//! Why this matters: `docs/diagrams/sprint-5/prove-pipeline.md` flips
//! from `source_of_truth: diagram` to `source_of_truth: code` at this same
//! E8 commit — drift gate 6 in the diagram-linter activates. Any
//! accidental `pub` addition / rename / signature change in `lib.rs`
//! shifts the golden diff, the linter's reverse-reference check catches
//! missing diagram entries, and this test catches the symmetric case
//! where Rust code grew a `pub` that the diagram doesn't yet reflect.
//!
//! Multi-line `pub use` extension (one delta from the fingerprint
//! precedent): `attestrum-prove`'s lib.rs re-exports 16 symbols from
//! `attestrum-attest` via a multi-line `pub use attestrum_attest::{ ... };`
//! block. The fingerprint parser only scans line-by-line, so it would
//! collapse the whole block into one entry. This crate's parser pre-passes
//! the file to flatten multi-line use-blocks into per-symbol synthetic
//! lines, so the golden carries one entry per re-exported symbol —
//! catching drift inside the block.

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
            "read golden {}: {e}\nHint: ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-prove --test api_surface",
            golden_path.display()
        )
    });

    if actual_text != expected {
        let expected_set: BTreeSet<&str> = expected.lines().filter(|l| !l.is_empty()).collect();
        let actual_set: BTreeSet<&str> = actual_text.lines().filter(|l| !l.is_empty()).collect();
        let added: Vec<&&str> = actual_set.difference(&expected_set).collect();
        let removed: Vec<&&str> = expected_set.difference(&actual_set).collect();
        let mut msg = String::from("attestrum-prove public API surface differs from golden.\n");
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
             ATTESTRUM_REGEN_API_SURFACE=1 cargo test -p attestrum-prove --test api_surface api_surface_matches_golden_file\n\
             and update docs/diagrams/sprint-5/prove-pipeline.md to reflect the new API.\n",
        );
        panic!("{msg}");
    }
}

#[test]
fn collect_api_surface_includes_prove_entry_point() {
    let actual = collect_api_surface(crate_dir());
    assert!(
        actual.iter().any(|l| l.contains("fn prove")),
        "fn prove not in surface — the v0.1 library entry point is required"
    );
}

#[test]
fn collect_api_surface_includes_six_proof_target_path_types() {
    // The four public-API container types that callers construct directly.
    // PerceptualHashes is exposed because the Perceptual variant carries it
    // by value (callers without an inline fingerprinter need to construct
    // it manually).
    let actual = collect_api_surface(crate_dir());
    for type_name in [
        "enum ProofTarget",
        "enum ManifestSource",
        "struct ProveOpts",
        "struct ProofArtifact",
        "enum ProofKind",
        "struct PerceptualHashes",
    ] {
        assert!(
            actual.iter().any(|l| l.contains(type_name)),
            "{type_name} not in surface — required by the v0.1 API contract"
        );
    }
}

#[test]
fn collect_api_surface_includes_six_variant_error() {
    // PATH-A-BRIEF §2.2 locks AttestrumProveError to exactly 6 variants
    // through v0.1. The presence of the enum in the surface is the
    // minimum check; variant-level coverage is enforced by the
    // `AttestrumProveError` non-exhaustive match in
    // `attestrum-cli/src/commands/prove.rs::map_error_to_exit_code`.
    let actual = collect_api_surface(crate_dir());
    assert!(
        actual
            .iter()
            .any(|l| l.contains("enum AttestrumProveError")),
        "enum AttestrumProveError not in surface — locks the v0.1 error contract"
    );
}

#[test]
fn collect_api_surface_includes_re_exports() {
    // Multi-line `pub use attestrum_attest::{ ... };` and single-line
    // `pub use attestrum_fingerprint::{ ... };` blocks expand to per-symbol
    // entries in the surface. Sample a handful that callers commonly
    // construct: InclusionProofPredicate (inclusion proof container),
    // NonInclusionProofPredicate (non-inclusion proof container), Subject
    // (subject of any attestation), FingerprintBundle (callers building
    // a ProofTarget::Bundle hand it to attestrum-prove from
    // attestrum-fingerprint).
    let actual = collect_api_surface(crate_dir());
    for symbol in [
        "use attestrum_attest::InclusionProofPredicate",
        "use attestrum_attest::NonInclusionProofPredicate",
        "use attestrum_attest::Subject",
        "use attestrum_fingerprint::FingerprintBundle",
    ] {
        assert!(
            actual.iter().any(|l| l.contains(symbol)),
            "{symbol} not in surface — multi-line pub use expander failed to flatten it"
        );
    }
}
