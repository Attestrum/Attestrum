//! `secret-scanner` — credential / private-key pattern scanner.
//!
//! Sixth pre-commit gate per CLAUDE.md §7. Scans staged content (or the working
//! tree with `--all`) for standard credential patterns: AWS / GitHub / Anthropic /
//! OpenAI / Stripe / Slack / Google / HuggingFace / npm / PEM private keys / JWT.
//!
//! Matches the `tools/diagram-linter/` pattern: thin CLI shim in `main.rs`,
//! check logic in this library, integration tests under `tests/`.
//!
//! **Scope (per CLAUDE.md §0.5.3)**: standard credential patterns only.
//! Project-specific PII rules (founder email / absolute-path leaks / other-
//! business domains) are NOT enforced by this scanner — they're policy in
//! CLAUDE.md §0.5.3 and rely on agent compliance. Reason: the false-positive
//! rate of those patterns is too high for a hard-block gate, and the policy
//! text + per-session re-read is the right enforcement layer for them.

use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

/// A credential pattern with a human-readable name.
pub struct Pattern {
    pub name: &'static str,
    pub regex: Regex,
}

/// A scanner finding: file path + line number + matched substring + pattern name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub pattern: &'static str,
    pub matched: String,
}

/// Build the standard pattern set. Patterns are anchored to catch real
/// credentials while minimizing false-positive rate. Each compiles into a
/// `Regex` once at startup.
pub fn standard_patterns() -> Vec<Pattern> {
    vec![
        Pattern {
            name: "aws-access-key-id",
            regex: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
        },
        Pattern {
            name: "anthropic-api-key",
            regex: Regex::new(r"sk-ant-[A-Za-z0-9_-]{20,}").unwrap(),
        },
        Pattern {
            name: "openai-api-key",
            regex: Regex::new(r"sk-(?:proj-)?[A-Za-z0-9]{20}T3BlbkFJ[A-Za-z0-9]{20}").unwrap(),
        },
        Pattern {
            name: "github-pat-classic",
            regex: Regex::new(r"ghp_[A-Za-z0-9]{36,}").unwrap(),
        },
        Pattern {
            name: "github-pat-server",
            regex: Regex::new(r"ghs_[A-Za-z0-9]{36,}").unwrap(),
        },
        Pattern {
            name: "github-pat-oauth",
            regex: Regex::new(r"gho_[A-Za-z0-9]{36,}").unwrap(),
        },
        Pattern {
            name: "github-pat-user",
            regex: Regex::new(r"ghu_[A-Za-z0-9]{36,}").unwrap(),
        },
        Pattern {
            name: "github-pat-refresh",
            regex: Regex::new(r"ghr_[A-Za-z0-9]{36,}").unwrap(),
        },
        Pattern {
            name: "github-pat-fine-grained",
            regex: Regex::new(r"github_pat_[A-Za-z0-9_]{82,}").unwrap(),
        },
        Pattern {
            name: "slack-token",
            regex: Regex::new(r"xox[bapors]-[A-Za-z0-9-]{10,}").unwrap(),
        },
        Pattern {
            name: "stripe-secret-key",
            regex: Regex::new(r"sk_(?:live|test)_[A-Za-z0-9]{24,}").unwrap(),
        },
        Pattern {
            name: "stripe-restricted-key",
            regex: Regex::new(r"rk_(?:live|test)_[A-Za-z0-9]{24,}").unwrap(),
        },
        Pattern {
            name: "stripe-publishable-key-live",
            regex: Regex::new(r"pk_live_[A-Za-z0-9]{24,}").unwrap(),
        },
        Pattern {
            name: "google-api-key",
            regex: Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap(),
        },
        Pattern {
            name: "huggingface-token",
            regex: Regex::new(r"hf_[A-Za-z0-9]{30,}").unwrap(),
        },
        Pattern {
            name: "npm-token",
            regex: Regex::new(r"npm_[A-Za-z0-9]{30,}").unwrap(),
        },
        Pattern {
            name: "pem-private-key",
            regex: Regex::new(r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----")
                .unwrap(),
        },
        Pattern {
            name: "jwt",
            regex: Regex::new(r"eyJ[A-Za-z0-9_-]{20,}\.eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}")
                .unwrap(),
        },
    ]
}

/// Files / directories the scanner skips entirely. These are known sources of
/// false positives (lockfiles with crate.io SHA256 checksums, build artifacts)
/// or contain the regex patterns themselves (the scanner's own source).
pub fn is_skipped(path: &Path) -> bool {
    let s = path.to_string_lossy();
    // Never scan the scanner's own source — patterns themselves would match.
    if s.contains("tools/secret-scanner/") {
        return true;
    }
    // Cargo.lock contains crate.io SHA-256 checksums (hex strings that aren't
    // credentials). The pattern set avoids generic hex but skip the file
    // entirely for clarity.
    if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.lock") {
        return true;
    }
    // Build artifacts + git internals + dirs already gitignored.
    let skip_prefixes = [
        "target/",
        ".git/",
        "diagrams-png/",
        ".attestrum/",
        ".claude/",
        ".playwright-mcp/",
        "Branding Material/",
    ];
    skip_prefixes
        .iter()
        .any(|p| s.starts_with(p) || s.contains(&format!("/{p}")))
}

/// Scan a single byte slice against all patterns. Returns one `Finding` per
/// (pattern, line) pair. Lines without matches are silent.
pub fn scan_bytes(path: &Path, content: &[u8], patterns: &[Pattern]) -> Vec<Finding> {
    // Treat content as best-effort UTF-8. Binary files with embedded text are
    // still scannable; truly binary content just won't match the regexes.
    let text = String::from_utf8_lossy(content);
    let mut out = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        for pat in patterns {
            if let Some(m) = pat.regex.find(line) {
                out.push(Finding {
                    path: path.to_path_buf(),
                    line: line_no + 1,
                    pattern: pat.name,
                    matched: m.as_str().to_string(),
                });
            }
        }
    }
    out
}

/// Read the staged content of a path from git's index (not the working tree).
/// This catches the case where someone stages secret content, edits the file
/// afterwards, and commits — the hook should see what's actually being
/// committed, not what's in the working tree.
pub fn read_staged_content(workspace_root: &Path, path: &Path) -> Result<Vec<u8>, String> {
    let spec = format!(":{}", path.to_string_lossy());
    let output = Command::new("git")
        .current_dir(workspace_root)
        .args(["show", &spec])
        .output()
        .map_err(|e| format!("git show {spec}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git show {spec}: exit {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// List paths staged for add/modify in the current index. Excludes deletions
/// (the deleted content can't be a leak from this commit since it's leaving).
pub fn staged_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .current_dir(workspace_root)
        .args(["diff", "--cached", "--name-only", "--diff-filter=AM"])
        .output()
        .map_err(|e| format!("git diff --cached: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git diff --cached exit {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// Walk the working tree for files to scan when `--all` is requested. Skips
/// binary files by best-effort detection (NUL byte in first 8 KiB).
pub fn walk_working_tree(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk(workspace_root, workspace_root, &mut out)?;
    Ok(out)
}

fn walk(workspace_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(workspace_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());
        if is_skipped(&rel) {
            continue;
        }
        if path.is_dir() {
            walk(workspace_root, &path, out)?;
        } else if path.is_file() {
            out.push(rel);
        }
    }
    Ok(())
}

/// Best-effort binary detection: check for a NUL byte in the first 8 KiB. If
/// any binary appears, skip the file — credential strings need to be
/// human-readable text to be useful, so binary content is overwhelmingly
/// false-positive territory.
pub fn looks_binary(content: &[u8]) -> bool {
    let sample_len = content.len().min(8192);
    content[..sample_len].contains(&0u8)
}

/// Scan a list of files (paths relative to `workspace_root`) using the given
/// patterns. Reads working-tree content. Returns all findings.
pub fn scan_paths_working_tree(
    workspace_root: &Path,
    paths: &[PathBuf],
    patterns: &[Pattern],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in paths {
        if is_skipped(path) {
            continue;
        }
        let abs = workspace_root.join(path);
        let Ok(content) = std::fs::read(&abs) else {
            continue;
        };
        if looks_binary(&content) {
            continue;
        }
        findings.extend(scan_bytes(path, &content, patterns));
    }
    findings
}

/// Scan staged paths using the git index (not the working tree). This is the
/// pre-commit hook mode.
pub fn scan_paths_staged(
    workspace_root: &Path,
    paths: &[PathBuf],
    patterns: &[Pattern],
) -> Result<Vec<Finding>, String> {
    let mut findings = Vec::new();
    for path in paths {
        if is_skipped(path) {
            continue;
        }
        let content = read_staged_content(workspace_root, path)?;
        if looks_binary(&content) {
            continue;
        }
        findings.extend(scan_bytes(path, &content, patterns));
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_access_key_matches() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new("test.txt"),
            b"const KEY = \"AKIAIOSFODNN7EXAMPLE\";",
            &patterns,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, "aws-access-key-id");
        assert_eq!(findings[0].matched, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn anthropic_api_key_matches() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new("config.py"),
            b"ANTHROPIC_KEY = 'sk-ant-api03-abcdefghijklmnop0123456789'",
            &patterns,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, "anthropic-api-key");
    }

    #[test]
    fn github_pat_classic_matches() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new(".env"),
            b"GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            &patterns,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, "github-pat-classic");
    }

    #[test]
    fn pem_private_key_matches() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new("server.key"),
            b"-----BEGIN RSA PRIVATE KEY-----\nMIIEpAI...rest...",
            &patterns,
        );
        assert!(!findings.is_empty());
        assert_eq!(findings[0].pattern, "pem-private-key");
    }

    #[test]
    fn jwt_three_segment_matches() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new("auth.log"),
            b"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4ifQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c",
            &patterns,
        );
        assert!(!findings.is_empty());
        assert_eq!(findings[0].pattern, "jwt");
    }

    #[test]
    fn cargo_lock_skipped() {
        assert!(is_skipped(Path::new("Cargo.lock")));
    }

    #[test]
    fn target_dir_skipped() {
        assert!(is_skipped(Path::new("target/debug/foo")));
    }

    #[test]
    fn scanner_own_source_skipped() {
        assert!(is_skipped(Path::new("tools/secret-scanner/src/lib.rs")));
    }

    #[test]
    fn ordinary_text_no_findings() {
        let patterns = standard_patterns();
        let findings = scan_bytes(
            Path::new("README.md"),
            b"# My Project\n\nA cryptographic provenance tool. See LICENSE.\n",
            &patterns,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn rejects_short_aws_lookalikes() {
        let patterns = standard_patterns();
        // 15-char suffix — not enough for AKIA[0-9A-Z]{16}.
        let findings = scan_bytes(Path::new("test.txt"), b"AKIA12345678901234X", &patterns);
        // Pattern requires 16 chars after AKIA; "12345678901234X" is 15.
        assert!(findings.is_empty(), "expected no match for short lookalike");
    }

    #[test]
    fn binary_content_detected() {
        let nul_at_start = b"\x00\x01\x02hello";
        assert!(looks_binary(nul_at_start));
        let ordinary_text = b"hello world this is fine";
        assert!(!looks_binary(ordinary_text));
    }
}
