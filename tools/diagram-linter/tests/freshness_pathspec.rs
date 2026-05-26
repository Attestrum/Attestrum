//! Integration tests for the pathspec-excluded freshness rolling window.
//!
//! These exercise `FreshnessOracle::from_git` against an ephemeral git
//! repository with a controlled commit sequence. They verify the
//! observable failure mode that triggered the fix: pure-docs commits
//! pushing pre-existing diagram SHAs out of the 30-commit window
//! despite no underlying code or diagram change.

use std::path::Path;
use std::process::Command;

use diagram_linter::{FreshnessOracle, DOCS_ONLY_EXCLUDES};
use tempfile::tempdir;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} in {repo:?}: {e}"));
    assert!(status.success(), "git {args:?} failed in {repo:?}");
}

fn git_init(repo: &Path) {
    git(repo, &["init", "--quiet", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.com"]);
    git(repo, &["config", "user.name", "test"]);
    // The protocol's pre-commit hook lives in .githooks/ in the real repo;
    // it's not present in these temp repos, so --no-verify on every commit
    // is robust even if the test environment picks up some hook config.
}

fn commit(repo: &Path, file: &str, content: &str, msg: &str) -> String {
    std::fs::write(repo.join(file), content).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-m", msg, "--no-verify", "--quiet"]);
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-parse failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn docs_only_commits_do_not_load_freshness_window() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);

    // Commit 1: the code commit a diagram would record as its `last_verified` SHA.
    let code_sha = commit(repo, "src.rs", "fn main() {}\n", "code commit");

    // Commits 2-31: pure docs-only churn on CHANGELOG.md. Without the
    // pathspec exclude these would push `code_sha` to position 31 (out of
    // a 30-commit window).
    for i in 0..30 {
        commit(
            repo,
            "CHANGELOG.md",
            &format!("entry {i}\n"),
            &format!("docs {i}"),
        );
    }

    // With the production exclude list, `code_sha` should still be in the window.
    let oracle = FreshnessOracle::from_git(repo, 30, DOCS_ONLY_EXCLUDES).unwrap();
    assert!(
        oracle.recent_shas.contains(&code_sha),
        "code SHA {code_sha} dropped out of window despite docs-only churn; \
         recent_shas={:?}",
        oracle.recent_shas
    );

    // Sanity-check the negative: passing empty excludes (pre-fix behavior)
    // SHOULD age `code_sha` out. If this control assertion fails the test
    // isn't actually exercising the intended boundary case.
    let oracle_legacy = FreshnessOracle::from_git(repo, 30, &[]).unwrap();
    assert!(
        !oracle_legacy.recent_shas.contains(&code_sha),
        "control: code SHA should be aged out under legacy (no-exclude) behavior"
    );
}

#[test]
fn mixed_code_and_docs_commit_still_counts() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);

    // A "mixed" commit — touched both src.rs AND CHANGELOG.md, the normal
    // shape of a CLAUDE.md §6 commit (release-relevant change + CHANGELOG
    // entry in the same commit). Pathspec matching is OR-based, so this
    // commit MUST count toward the window because it touched at least one
    // non-excluded file.
    std::fs::write(repo.join("src.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("CHANGELOG.md"), "entry\n").unwrap();
    git(repo, &["add", "src.rs", "CHANGELOG.md"]);
    git(repo, &["commit", "-m", "mixed", "--no-verify", "--quiet"]);
    let mixed_sha = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let oracle = FreshnessOracle::from_git(repo, 30, DOCS_ONLY_EXCLUDES).unwrap();
    assert!(
        oracle.recent_shas.contains(&mixed_sha),
        "mixed commit (touched code + CHANGELOG) must still count toward the window"
    );
}

#[test]
fn empty_excludes_matches_legacy_behavior() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);

    commit(repo, "src.rs", "fn a() {}\n", "one commit");

    let oracle = FreshnessOracle::from_git(repo, 30, &[]).unwrap();
    assert_eq!(
        oracle.recent_shas.len(),
        1,
        "empty excludes should reproduce legacy 'count every commit' behavior"
    );
}
