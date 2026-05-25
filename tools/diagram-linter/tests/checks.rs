//! Integration tests for `diagram-linter` Checks 1, 2, 3, and 5.
//!
//! Mermaid tests skip gracefully when `mmdc` is not on PATH and `ATTESTRUM_MMDC`
//! is unset, so `cargo test` works on a bare dev machine — they only fail
//! when `mmdc` is actually available but the diagram is broken.
//!
//! Each test passes `Some(fixture_dir)` as the workspace_root override so
//! Check 5 (reverse references) doesn't leak into the real Attestrum workspace's
//! pub items.

use std::path::PathBuf;

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn mmdc_available() -> bool {
    if std::env::var_os("ATTESTRUM_MMDC").is_some() {
        return true;
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join("mmdc").is_file()))
        .unwrap_or(false)
}

fn run_against(name: &str) -> diagram_linter::CheckReport {
    let dir = fixture_dir(name);
    diagram_linter::run_check(&dir, Some(&dir), false).expect("run_check should not error")
}

#[test]
fn ok_fixture_passes_frontmatter_check() {
    let report = run_against("ok");
    let fm_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "frontmatter")
        .collect();
    assert!(
        fm_failures.is_empty(),
        "ok fixture should have no frontmatter failures, got: {fm_failures:?}"
    );
}

#[test]
fn bad_frontmatter_fixture_reports_missing_key() {
    let report = run_against("bad-frontmatter");
    let fm_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "frontmatter")
        .collect();
    assert!(
        !fm_failures.is_empty(),
        "bad-frontmatter fixture should report at least one frontmatter failure"
    );
    let combined = fm_failures
        .iter()
        .map(|f| f.message.as_str())
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        combined.contains("last_verified"),
        "failure message should name `last_verified`, got: {combined}"
    );
}

#[test]
fn ok_fixture_passes_mermaid_check() {
    if !mmdc_available() {
        eprintln!("[skipped: mmdc not on PATH and ATTESTRUM_MMDC unset]");
        return;
    }
    let report = run_against("ok");
    let mermaid_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "mermaid")
        .collect();
    assert!(
        mermaid_failures.is_empty(),
        "ok fixture should parse cleanly under mmdc, got: {mermaid_failures:?}"
    );
}

#[test]
fn bad_mermaid_fixture_fails_mermaid_check() {
    if !mmdc_available() {
        eprintln!("[skipped: mmdc not on PATH and ATTESTRUM_MMDC unset]");
        return;
    }
    let report = run_against("bad-mermaid");
    let mermaid_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "mermaid")
        .collect();
    assert!(
        !mermaid_failures.is_empty(),
        "bad-mermaid fixture should produce ≥1 mermaid failure under mmdc"
    );
}

#[test]
fn ok_fixture_passes_freshness_check() {
    // The ok fixture uses `source_of_truth: diagram` + `last_verified: bootstrap 2026-05-23`,
    // which Check 3 accepts unconditionally.
    let report = run_against("ok");
    let freshness_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "freshness")
        .collect();
    assert!(
        freshness_failures.is_empty(),
        "ok fixture (bootstrap token + source_of_truth: diagram) should pass Check 3, got: {freshness_failures:?}"
    );
}

#[test]
fn ok_fixture_passes_reverse_ref_check() {
    // Fixture workspace has no crates/ directory → Check 5 trivially passes.
    let report = run_against("ok");
    let rr_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|f| f.check == "reverse-ref")
        .collect();
    assert!(
        rr_failures.is_empty(),
        "ok fixture (no crates) should pass Check 5 trivially, got: {rr_failures:?}"
    );
}

#[test]
fn ok_fixture_report_is_clean_when_mmdc_available() {
    if !mmdc_available() {
        eprintln!("[skipped: mmdc not on PATH and ATTESTRUM_MMDC unset]");
        return;
    }
    let report = run_against("ok");
    // checks_run for ok fixture (1 valid file):
    //   Per-file: Check 2 (frontmatter) + Check 3 (freshness) + Check 1 (mermaid)  = 3
    //   Workspace-once: Check 5 (reverse-ref) + Check 4 (forward-ref) + Check 6 (drift) = 3
    // = 6 total.
    assert_eq!(report.checks_run, 6, "expected 6 checks for ok fixture");
    assert_eq!(
        report.failures.len(),
        0,
        "ok fixture should be entirely clean when mmdc is available, got: {:?}",
        report.failures
    );
}
