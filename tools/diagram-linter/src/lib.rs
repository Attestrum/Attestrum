//! `diagram-linter` library — the check implementations live here so that
//! integration tests under `tests/` can exercise them directly without
//! spawning a child process per case.
//!
//! Sprint 1 commit E4 ships Check 1 (Mermaid parse via `mmdc`) and Check 2
//! (frontmatter has the four required keys). Commits E5 + E6 layer the
//! remaining four checks on top per PATH-A-BRIEF §0.3.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The four frontmatter keys every `docs/diagrams/**/*.md` file MUST carry.
pub const REQUIRED_FRONTMATTER_KEYS: &[&str] =
    &["title", "models", "source_of_truth", "last_verified"];

#[derive(Debug, Clone)]
pub struct Failure {
    pub file: PathBuf,
    pub check: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct CheckReport {
    pub checks_run: usize,
    pub failures: Vec<Failure>,
    pub strict: bool,
}

impl CheckReport {
    pub fn new(strict: bool) -> Self {
        Self {
            checks_run: 0,
            failures: Vec::new(),
            strict,
        }
    }

    pub fn record_check(&mut self) {
        self.checks_run += 1;
    }

    pub fn record_failure(&mut self, file: PathBuf, check: &'static str, message: String) {
        self.failures.push(Failure {
            file,
            check,
            message,
        });
    }

    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn exit_code(&self) -> u8 {
        if self.ok() {
            0
        } else {
            1
        }
    }

    pub fn print_human(&self) {
        println!(
            "diagram-linter: {} checks run, {} failure(s){}",
            self.checks_run,
            self.failures.len(),
            if self.strict { " [strict]" } else { "" }
        );
        for f in &self.failures {
            println!("  FAIL [{}] {}: {}", f.check, f.file.display(), f.message);
        }
    }

    pub fn print_json(&self) {
        let failures: Vec<String> = self
            .failures
            .iter()
            .map(|f| {
                format!(
                    "{{\"file\":\"{}\",\"check\":\"{}\",\"message\":\"{}\"}}",
                    escape_json(&f.file.to_string_lossy()),
                    f.check,
                    escape_json(&f.message)
                )
            })
            .collect();
        println!(
            "{{\"checks_run\":{},\"failures\":[{}],\"strict\":{}}}",
            self.checks_run,
            failures.join(","),
            self.strict
        );
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Run all enabled checks against every `*.md` file under `root` (recursively).
///
/// `workspace_root` is an optional override; when `None`, the linter walks up
/// from `root` to find the nearest `Cargo.toml` with a `[workspace]` section.
/// Integration tests pointing at fixture directories pass `Some(fixture_root)`
/// to isolate Check 5 from the real workspace's `pub` items.
///
/// E4 enables Check 1 (Mermaid parse) and Check 2 (frontmatter complete).
/// E5 adds Check 3 (last_verified freshness) and Check 5 (reverse references).
/// Returns the populated [`CheckReport`].
pub fn run_check(
    root: &Path,
    workspace_root: Option<&Path>,
    strict: bool,
) -> Result<CheckReport, String> {
    let files = walk_markdown(root)?;
    let mut report = CheckReport::new(strict);

    let workspace_root = workspace_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| find_workspace_root(root));
    let oracle = FreshnessOracle::from_git(&workspace_root, 30)?;

    for file in &files {
        let content =
            std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;

        // Check 2: frontmatter complete.
        report.record_check();
        let (fm_result, body) = check_frontmatter(&content);
        if let Err(msg) = fm_result {
            report.record_failure(file.clone(), "frontmatter", msg);
            // Skip Check 3 if frontmatter is malformed — it would just re-report.
        } else {
            // Check 3: last_verified freshness (with bootstrap exception).
            report.record_check();
            if let Err(msg) = check_freshness(&content, &oracle) {
                report.record_failure(file.clone(), "freshness", msg);
            }
        }

        // Check 1: every ```mermaid block parses via `mmdc`.
        report.record_check();
        if let Err(msg) = check_mermaid_blocks(body) {
            report.record_failure(file.clone(), "mermaid", msg);
        }
    }

    // Check 5: every workspace `pub` item is referenced by ≥1 diagram.
    // Run once per workspace, not per diagram file.
    report.record_check();
    let pub_items = gather_workspace_pub_items(&workspace_root)?;
    if !pub_items.is_empty() {
        let blob = load_all_diagram_text(root)?;
        let unreferenced = find_unreferenced_pub_items(&pub_items, &blob);
        for item in unreferenced {
            report.record_failure(
                item.file.clone(),
                "reverse-ref",
                format!(
                    "pub {} `{}` is not referenced by any diagram (add to a `models:` field or a Mermaid node label)",
                    item.kind, item.name
                ),
            );
        }
    }

    // Check 4: every `models:` token resolves (file path on disk, known pub item,
    // or external URI). Runs once over all diagrams; per-file failures are recorded
    // individually.
    report.record_check();
    let pub_item_names: HashSet<String> = pub_items.iter().map(|i| i.name.clone()).collect();
    let dangling = check_forward_refs(root, &workspace_root, &pub_item_names)?;
    for (file, token) in dangling {
        report.record_failure(
            file,
            "forward-ref",
            format!(
                "models: token `{token}` does not resolve to a file, pub item, or external URI"
            ),
        );
    }

    // Check 6: drift (code file changed but referencing diagram wasn't).
    // Runs once per check invocation; uses staged-set (or ATTESTRUM_DRIFT_BASE).
    report.record_check();
    let changed = git_changed_files(&workspace_root)?;
    if !changed.is_empty() {
        let drifts = check_drift(root, &workspace_root, &changed)?;
        for drift in drifts {
            report.record_failure(drift.diagram.clone(), "drift", drift.message);
        }
    }

    Ok(report)
}

/// Find the workspace root by walking up from `start` until we hit a `Cargo.toml`
/// that declares `[workspace]`. Falls back to `start` itself if not found.
fn find_workspace_root(start: &Path) -> PathBuf {
    let mut cur = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        let cargo = cur.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(content) = std::fs::read_to_string(&cargo) {
                if content.contains("[workspace]") {
                    return cur;
                }
            }
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
}

/// Recursively collect every `*.md` file under `root` (skipping `target/`,
/// `node_modules/`, `.git/`).
pub fn walk_markdown(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk_recursive(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if path.is_dir() {
            // Skip noisy directories.
            if matches!(name.as_ref(), "target" | "node_modules" | ".git" | ".cargo") {
                continue;
            }
            walk_recursive(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

// ============================================================================
// Check 2 — frontmatter complete
// ============================================================================

/// Parse the YAML frontmatter at the top of a Markdown file and verify the four
/// required keys are present. Returns `(check_result, body_after_frontmatter)`.
///
/// We don't pull in a full YAML parser at Sprint 1 — frontmatter is `---`-delimited
/// and we only need to detect a fixed set of top-level keys. Value validation
/// (e.g., `source_of_truth ∈ {code, diagram, spec}`) lands in E5 alongside the
/// freshness check.
pub fn check_frontmatter(content: &str) -> (Result<(), String>, &str) {
    let mut lines = content.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (Err("file is empty".into()), content);
    };
    if first.trim_end() != "---" {
        return (
            Err("missing frontmatter opening delimiter (---) on line 1".into()),
            content,
        );
    }
    let mut byte_offset = first.len();
    let mut keys: HashSet<String> = HashSet::new();
    let mut found_close = false;
    for line in lines {
        byte_offset += line.len();
        if line.trim_end() == "---" {
            found_close = true;
            break;
        }
        // YAML key extraction: anything before the first `:` on a non-indented line.
        // Indented lines (continuations / nested values) are ignored for required-key
        // detection.
        if let Some((key_part, _value)) = line.split_once(':') {
            if !key_part.starts_with(' ') && !key_part.starts_with('\t') {
                keys.insert(key_part.trim().to_string());
            }
        }
    }
    if !found_close {
        return (
            Err("missing frontmatter closing delimiter (---)".into()),
            content,
        );
    }
    let body = &content[byte_offset..];
    let missing: Vec<&str> = REQUIRED_FRONTMATTER_KEYS
        .iter()
        .copied()
        .filter(|k| !keys.contains(*k))
        .collect();
    if !missing.is_empty() {
        return (
            Err(format!(
                "missing required frontmatter key(s): {}",
                missing.join(", ")
            )),
            body,
        );
    }
    (Ok(()), body)
}

// ============================================================================
// Check 1 — every ```mermaid block parses via `mmdc`
// ============================================================================

/// Extract and validate every ```` ```mermaid ```` fenced code block in `body`.
/// Returns `Ok(())` if all blocks parse; otherwise an error describing the
/// first failure (with `mmdc`'s stderr).
///
/// Resolution order for the `mmdc` binary:
///   1. `ATTESTRUM_MMDC` env var (absolute path).
///   2. `mmdc` on `PATH` (the canonical install per PATH-A-BRIEF §10.1 step 4).
///   3. `npx -y @mermaid-js/mermaid-cli@10.9.1` fallback.
pub fn check_mermaid_blocks(body: &str) -> Result<(), String> {
    let blocks = extract_mermaid_blocks(body);
    if blocks.is_empty() {
        return Err("no ```mermaid``` fenced code block found".into());
    }
    let runner = MmdcRunner::resolve()?;
    for (idx, block) in blocks.iter().enumerate() {
        runner.parse_block(idx, block)?;
    }
    Ok(())
}

fn extract_mermaid_blocks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed == "```mermaid" || trimmed.starts_with("```mermaid ") {
            let mut block = String::new();
            for content_line in lines.by_ref() {
                if content_line.trim_start().starts_with("```") {
                    break;
                }
                block.push_str(content_line);
                block.push('\n');
            }
            out.push(block);
        }
    }
    out
}

struct MmdcRunner {
    program: String,
    base_args: Vec<String>,
}

impl MmdcRunner {
    fn resolve() -> Result<Self, String> {
        if let Ok(path) = std::env::var("ATTESTRUM_MMDC") {
            return Ok(Self {
                program: path,
                base_args: Vec::new(),
            });
        }
        if let Some(path) = find_on_path("mmdc") {
            return Ok(Self {
                program: path.to_string_lossy().into_owned(),
                base_args: Vec::new(),
            });
        }
        if let Some(path) = find_on_path("npx") {
            return Ok(Self {
                program: path.to_string_lossy().into_owned(),
                base_args: vec!["-y".into(), "@mermaid-js/mermaid-cli@10.9.1".into()],
            });
        }
        Err("mmdc not found on PATH and `npx` unavailable. \
             Install with: npm install -g @mermaid-js/mermaid-cli@10.9.1"
            .into())
    }

    fn parse_block(&self, idx: usize, block: &str) -> Result<(), String> {
        // Write to a process-unique temp file; mmdc requires a file output even when
        // we only care about whether it parses.
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out_path =
            std::env::temp_dir().join(format!("attestrum-linter-{pid}-{nanos}-{idx}.svg"));

        let mut cmd = Command::new(&self.program);
        cmd.args(&self.base_args);
        cmd.args(["-i", "-", "-o"]).arg(&out_path);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn mmdc ({}): {e}", self.program))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(block.as_bytes())
                .map_err(|e| format!("write mmdc stdin: {e}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| format!("wait mmdc: {e}"))?;
        // Cleanup — ignore errors (file may not exist if mmdc failed before writing).
        let _ = std::fs::remove_file(&out_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "block {idx}: mmdc parse failed (exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            ));
        }
        Ok(())
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(binary);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

// ============================================================================
// Frontmatter value extraction (shared by Checks 3 and beyond)
// ============================================================================

/// Extract the trimmed value of a single top-level frontmatter key.
/// Returns `None` if the key is missing or the file has no frontmatter.
/// Strips surrounding `"` from quoted values.
pub fn extract_frontmatter_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let mut lines = content.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" {
            return None;
        }
        if let Some((k, v)) = line.split_once(':') {
            if !k.starts_with(' ') && !k.starts_with('\t') && k.trim() == key {
                let v = v.trim();
                // Strip surrounding double quotes if present (cheap; no full YAML).
                let stripped = v
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(v);
                return Some(stripped);
            }
        }
    }
    None
}

// ============================================================================
// Check 3 — last_verified freshness (with bootstrap exception)
// ============================================================================

/// A snapshot of the recent commit window plus the rule for when the
/// `bootstrap YYYY-MM-DD` token is still acceptable.
///
/// `source_of_truth: diagram` and `source_of_truth: spec` diagrams may carry
/// the bootstrap token indefinitely — they describe contracted-but-not-yet-implemented
/// or externally-authoritative state. `source_of_truth: code` diagrams MUST
/// carry a real short-SHA from the recent commit window.
pub struct FreshnessOracle {
    pub recent_shas: HashSet<String>,
}

impl FreshnessOracle {
    /// Build an oracle by shelling out to `git log` in `repo_root`. `window` controls
    /// how many recent commits are accepted (PATH-A-BRIEF §0.3 specifies 30).
    pub fn from_git(repo_root: &Path, window: usize) -> Result<Self, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["log", "--pretty=format:%h", "-n"])
            .arg(window.to_string())
            .output()
            .map_err(|e| format!("git log failed: {e}"))?;
        if !output.status.success() {
            // No git repo, or no commits yet. Treat as empty window — bootstrap-only
            // diagrams still pass; SHA-bearing diagrams fail closed.
            return Ok(Self {
                recent_shas: HashSet::new(),
            });
        }
        let recent_shas: HashSet<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self { recent_shas })
    }

    /// Construct directly from a set of SHAs — for tests, mostly.
    pub fn from_set(recent_shas: HashSet<String>) -> Self {
        Self { recent_shas }
    }
}

/// Verify the `last_verified` value against the freshness oracle. Reads
/// `source_of_truth` from the same content to decide whether the `bootstrap`
/// token is permissible.
pub fn check_freshness(content: &str, oracle: &FreshnessOracle) -> Result<(), String> {
    let lv = extract_frontmatter_value(content, "last_verified")
        .ok_or_else(|| "last_verified not found in frontmatter".to_string())?;
    let sot = extract_frontmatter_value(content, "source_of_truth")
        .ok_or_else(|| "source_of_truth not found in frontmatter".to_string())?;

    if let Some(rest) = lv.strip_prefix("bootstrap ") {
        return match sot {
            "diagram" | "spec" => Ok(()),
            "code" => Err(format!(
                "source_of_truth: code requires a real short-SHA in last_verified, \
                 not 'bootstrap {rest}' — refresh the diagram and bump last_verified \
                 to the SHA of the commit verifying it against current code"
            )),
            other => Err(format!(
                "invalid source_of_truth value: {other:?} (allowed: code, diagram, spec)"
            )),
        };
    }

    let mut parts = lv.split_whitespace();
    let sha = parts
        .next()
        .ok_or_else(|| format!("empty last_verified value: {lv:?}"))?;
    let _date = parts.next().ok_or_else(|| {
        format!("last_verified missing date: {lv:?} (expected: <short-sha> YYYY-MM-DD)")
    })?;
    if !oracle.recent_shas.contains(sha) {
        return Err(format!(
            "last_verified SHA '{sha}' not in recent {} commits — re-verify this \
             diagram against current code and bump the SHA",
            oracle.recent_shas.len()
        ));
    }
    Ok(())
}

// ============================================================================
// Check 5 — reverse references (every `pub` item is named by ≥1 diagram)
// ============================================================================

/// A `pub` item discovered in a workspace crate's source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubItem {
    pub name: String,
    pub kind: &'static str, // "mod" | "struct" | "trait" | "fn" | "enum" | "type" | "const" | "static"
    pub file: PathBuf,
}

/// Scan a Rust source file for top-level `pub` items, skipping anything
/// preceded by `#[doc(hidden)]` on the immediately prior non-blank line.
///
/// Sprint 1 scope: only scans the top level (depth 0 in the brace tree).
/// Nested `pub` items inside `mod foo { ... }` blocks are NOT extracted —
/// they require richer parsing and are deferred until first need (Sprint 2+
/// when attestrum-merkle / attestrum-cas start having nested modules).
pub fn scan_pub_items(source_path: &Path) -> Result<Vec<PubItem>, String> {
    let content = std::fs::read_to_string(source_path)
        .map_err(|e| format!("read {}: {e}", source_path.display()))?;
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut prev_doc_hidden = false;
    let mut prev_cfg_test = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        // Track depth via a coarse brace count (ignores braces inside strings/comments;
        // good enough for our crate stubs and idiomatic code).
        let opens = line.matches('{').count() as i32;
        let closes = line.matches('}').count() as i32;

        if depth == 0 && !prev_doc_hidden && !prev_cfg_test {
            if let Some(item) = parse_top_level_pub(line, source_path) {
                out.push(item);
            }
        }
        depth += opens - closes;
        if depth < 0 {
            depth = 0;
        }

        // Update flags for next iteration based on THIS line.
        prev_doc_hidden = line == "#[doc(hidden)]";
        prev_cfg_test = line.starts_with("#[cfg(test)]");
    }
    Ok(out)
}

fn parse_top_level_pub(line: &str, source_path: &Path) -> Option<PubItem> {
    let rest = line.strip_prefix("pub ")?;
    // Drop visibility modifiers like `pub(crate)`, `pub(super)` — we already matched `pub ` so this is just for `pub(...)` shapes.
    let rest = rest.trim_start();
    for (kw, kind) in [
        ("mod ", "mod"),
        ("struct ", "struct"),
        ("trait ", "trait"),
        ("fn ", "fn"),
        ("enum ", "enum"),
        ("type ", "type"),
        ("const ", "const"),
        ("static ", "static"),
    ] {
        if let Some(after) = rest.strip_prefix(kw) {
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(PubItem {
                    name,
                    kind,
                    file: source_path.to_path_buf(),
                });
            }
        }
    }
    None
}

/// Walk `<workspace_root>/crates/**/src/{lib.rs,**/mod.rs}` and gather every
/// top-level `pub` item. Tooling crates under `tools/` are out of scope per
/// PATH-A-BRIEF §0.3's literal wording ("every `pub` item in `crates/**`").
pub fn gather_workspace_pub_items(workspace_root: &Path) -> Result<Vec<PubItem>, String> {
    let crates_dir = workspace_root.join("crates");
    if !crates_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&crates_dir)
        .map_err(|e| format!("read_dir {}: {e}", crates_dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for crate_dir in entries {
        if !crate_dir.is_dir() {
            continue;
        }
        let src_dir = crate_dir.join("src");
        let lib_rs = src_dir.join("lib.rs");
        if lib_rs.is_file() {
            out.extend(scan_pub_items(&lib_rs)?);
            // Also walk any */mod.rs under src/.
            collect_mod_rs(&src_dir, &mut out)?;
        }
    }
    Ok(out)
}

fn collect_mod_rs(dir: &Path, out: &mut Vec<PubItem>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_mod_rs(&path, out)?;
        } else if path.file_name().and_then(|s| s.to_str()) == Some("mod.rs") {
            out.extend(scan_pub_items(&path)?);
        }
    }
    Ok(())
}

/// Concatenate the full text of every diagram under `diagrams_root` into one big
/// blob, for substring searching in the reverse-reference check. Cached because
/// we re-use it once per `pub` item.
pub fn load_all_diagram_text(diagrams_root: &Path) -> Result<String, String> {
    let files = walk_markdown(diagrams_root)?;
    let mut blob = String::new();
    for file in files {
        let content =
            std::fs::read_to_string(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
        blob.push_str(&content);
        blob.push('\n');
    }
    Ok(blob)
}

/// For each pub item, verify its name appears at least once in `diagram_blob`.
/// Returns the list of items with no reference. Word-boundary match (the name
/// can't be part of a longer identifier) to reduce false positives.
pub fn find_unreferenced_pub_items(items: &[PubItem], diagram_blob: &str) -> Vec<PubItem> {
    items
        .iter()
        .filter(|item| !contains_as_word(diagram_blob, &item.name))
        .cloned()
        .collect()
}

fn contains_as_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut search_from = 0;
    while let Some(idx) = haystack[search_from..].find(needle) {
        let abs = search_from + idx;
        let before_ok = abs == 0
            || !haystack.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && haystack.as_bytes()[abs - 1] != b'_';
        let end = abs + needle.len();
        let after_ok = end >= haystack.len()
            || !haystack.as_bytes()[end].is_ascii_alphanumeric()
                && haystack.as_bytes()[end] != b'_';
        if before_ok && after_ok {
            return true;
        }
        search_from = abs + needle.len();
    }
    false
}

// ============================================================================
// Check 4 — forward references (every cited symbol/file exists in workspace)
// ============================================================================

/// Extract identifier-shaped tokens from a diagram's `models:` field. We split
/// on `,` and `;`, trim whitespace, and keep entries that look like file paths
/// (contain `/`) or `::`-qualified identifiers. Pure prose is skipped.
pub fn extract_models_tokens(models_value: &str) -> Vec<String> {
    models_value
        .split([',', ';'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        // Strip a trailing `::method` for path-shaped tokens so we can verify the file.
        .map(|s| {
            if let Some(idx) = s.find("::") {
                // Keep both "path/file.rs" and the full "path/file.rs::method" — the
                // file part is what we check on disk; the full token is checked via
                // the workspace pub items set.
                let path = &s[..idx];
                if path.contains('/') {
                    path.to_string()
                } else {
                    s
                }
            } else {
                s
            }
        })
        .collect()
}

/// Verify each `models:` token resolves to either:
///   - a real file path inside `workspace_root`, OR
///   - an external URL (`https://...` / `http://...`), OR
///   - a known workspace `pub` item name (for `::Item`-shaped tokens), OR
///   - a domain-rooted URI like `attestrum.com/...` (treated as external).
///
/// **Only enforced for `source_of_truth: code` diagrams.** `source_of_truth: diagram`
/// and `spec` diagrams describe contracted-future or externally-authoritative state;
/// their `models:` paths may legitimately not exist yet. The check kicks in once
/// the diagram flips to `code` in the same commit as the implementation lands.
///
/// Returns the list of (diagram, token) pairs that resolve to none of the above.
pub fn check_forward_refs(
    diagrams_root: &Path,
    workspace_root: &Path,
    pub_item_names: &HashSet<String>,
) -> Result<Vec<(PathBuf, String)>, String> {
    let files = walk_markdown(diagrams_root)?;
    let mut bad: Vec<(PathBuf, String)> = Vec::new();
    for file in files {
        let content =
            std::fs::read_to_string(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
        if extract_frontmatter_value(&content, "source_of_truth") != Some("code") {
            continue;
        }
        let Some(models) = extract_frontmatter_value(&content, "models") else {
            continue;
        };
        for token in extract_models_tokens(models) {
            if token_resolves(&token, workspace_root, pub_item_names) {
                continue;
            }
            bad.push((file.clone(), token));
        }
    }
    Ok(bad)
}

fn token_resolves(token: &str, workspace_root: &Path, pub_items: &HashSet<String>) -> bool {
    // External URIs / domain-rooted identifiers (e.g. `attestrum.com/...`,
    // `https://...`, `Sigstore Bundle v0.3`) are accepted.
    if token.starts_with("http://")
        || token.starts_with("https://")
        || token.contains('.') && !token.contains('/') && !token.contains("::")
        || token.starts_with("attestrum.com/")
    {
        return true;
    }
    // Treat tokens with whitespace as free-form descriptions (e.g., "Cargo.toml workspace + per-crate manifests").
    if token.contains(' ') {
        return true;
    }
    // File-path-shaped: must exist on disk relative to workspace_root.
    if token.contains('/') {
        // Strip a trailing `/` (directory references like `crates/attestrum-core/`).
        let candidate = workspace_root.join(token.trim_end_matches('/'));
        return candidate.exists();
    }
    // Bare-identifier shape (e.g., `AttestrumError`): must appear in workspace pub items.
    pub_items.contains(token)
}

// ============================================================================
// Check 6 — drift (code file changed but its referencing diagram wasn't)
// ============================================================================

/// A drift finding: a code file changed in this commit/PR without its
/// referencing diagram(s) being touched in the same change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftFinding {
    pub diagram: PathBuf,
    pub code_file: PathBuf,
    pub message: String,
}

/// For every `source_of_truth: code` diagram, check that any code file named in
/// its `models:` field is in `changed_files` IFF the diagram itself is also in
/// `changed_files`. A diff-set asymmetry is a drift.
///
/// `changed_files` should be paths relative to `workspace_root` OR absolute paths
/// (we normalize via `workspace_root.join(path)` if relative).
pub fn check_drift(
    diagrams_root: &Path,
    workspace_root: &Path,
    changed_files: &HashSet<PathBuf>,
) -> Result<Vec<DriftFinding>, String> {
    let files = walk_markdown(diagrams_root)?;
    let mut findings = Vec::new();
    // Normalize changed_files to absolute paths so we can compare.
    let changed_abs: HashSet<PathBuf> = changed_files
        .iter()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                workspace_root.join(p)
            }
        })
        .collect();
    for diagram in &files {
        let content = std::fs::read_to_string(diagram)
            .map_err(|e| format!("read {}: {e}", diagram.display()))?;
        if extract_frontmatter_value(&content, "source_of_truth") != Some("code") {
            continue;
        }
        let Some(models) = extract_frontmatter_value(&content, "models") else {
            continue;
        };
        // Normalize the diagram path to absolute before comparing against
        // `changed_abs` (which is always absolute). Without this, invoking the
        // linter via `--root docs/diagrams` (a relative path) produces relative
        // diagram paths from walk_markdown, while changed_abs is absolute, so
        // the contains() check always returns false and the drift check fires
        // even when the diagram IS staged. Surfaced in 2026-05-25 protocol
        // commit 4 (the first commit to ever change a models:-referenced code
        // file AND its referencing diagram in the same commit under CLI mode).
        let diagram_abs = if diagram.is_absolute() {
            diagram.clone()
        } else {
            workspace_root.join(diagram)
        };
        let diagram_changed = changed_abs.contains(&diagram_abs);
        for token in extract_models_tokens(models) {
            if !token.contains('/') {
                // Bare identifier — drift detection skips it (we only track file-level drift).
                continue;
            }
            let code_path = workspace_root.join(&token);
            if changed_abs.contains(&code_path) && !diagram_changed {
                findings.push(DriftFinding {
                    diagram: diagram.clone(),
                    code_file: code_path.clone(),
                    message: format!(
                        "code file {} changed but referencing diagram {} (source_of_truth: code) was not staged/changed in the same commit",
                        token,
                        diagram.display()
                    ),
                });
            }
        }
    }
    Ok(findings)
}

/// Convenience wrapper around `git diff --name-only`. Honors `ATTESTRUM_DRIFT_BASE`
/// env var (e.g., `origin/main`) for PR-base comparison; otherwise compares the
/// staged set (`git diff --cached --name-only`).
pub fn git_changed_files(workspace_root: &Path) -> Result<HashSet<PathBuf>, String> {
    let mut cmd = if let Ok(base) = std::env::var("ATTESTRUM_DRIFT_BASE") {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(workspace_root)
            .args(["diff", "--name-only"])
            .arg(format!("{base}...HEAD"));
        c
    } else {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(workspace_root)
            .args(["diff", "--cached", "--name-only"]);
        c
    };
    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Ok(HashSet::new()),
    };
    if !output.status.success() {
        return Ok(HashSet::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| workspace_root.join(l.trim()))
        .collect())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Check 2 (frontmatter) -----------------------------------------------

    #[test]
    fn frontmatter_ok_with_all_four_keys() {
        let content = "---\n\
                       title: foo\n\
                       models: bar\n\
                       source_of_truth: diagram\n\
                       last_verified: bootstrap 2026-05-23\n\
                       ---\n\
                       body here\n";
        let (result, body) = check_frontmatter(content);
        assert!(result.is_ok(), "got {result:?}");
        assert_eq!(body, "body here\n");
    }

    #[test]
    fn frontmatter_missing_one_key() {
        let content = "---\n\
                       title: foo\n\
                       models: bar\n\
                       source_of_truth: diagram\n\
                       ---\n\
                       body\n";
        let (result, _) = check_frontmatter(content);
        let err = result.unwrap_err();
        assert!(err.contains("last_verified"), "got {err}");
    }

    #[test]
    fn frontmatter_missing_open() {
        let (result, _) = check_frontmatter("no frontmatter here\n");
        assert!(result.is_err());
    }

    #[test]
    fn frontmatter_missing_close() {
        let content = "---\n\
                       title: foo\n\
                       models: bar\n\
                       source_of_truth: diagram\n\
                       last_verified: bootstrap 2026-05-23\n\
                       body never closes the FM\n";
        let (result, _) = check_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn frontmatter_ignores_indented_continuations() {
        // Multi-line YAML value (single-line really, but with extra-key noise).
        let content = "---\n\
                       title: foo\n\
                       models: |\n  some/path::Item\n  another/path::Item\n\
                       source_of_truth: spec\n\
                       last_verified: abc1234 2026-05-23\n\
                       ---\n\
                       body\n";
        let (result, _) = check_frontmatter(content);
        assert!(result.is_ok(), "got {result:?}");
    }

    // --- Check 1 (mermaid extraction) ----------------------------------------

    #[test]
    fn extracts_single_mermaid_block() {
        let body = "intro\n\n```mermaid\nflowchart TD\n  A --> B\n```\noutro\n";
        let blocks = extract_mermaid_blocks(body);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("flowchart TD"));
    }

    #[test]
    fn extracts_multiple_mermaid_blocks() {
        let body =
            "```mermaid\ngraph TD\nA-->B\n```\n\n```mermaid\nsequenceDiagram\nA->>B: hi\n```\n";
        let blocks = extract_mermaid_blocks(body);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn ignores_non_mermaid_fences() {
        let body = "```rust\nfn main(){}\n```\n\n```\nplain code\n```\n";
        assert_eq!(extract_mermaid_blocks(body).len(), 0);
    }

    // --- extract_frontmatter_value -------------------------------------------

    #[test]
    fn extract_value_works_on_unquoted() {
        let c = "---\nfoo: bar\nbaz: qux\n---\nbody\n";
        assert_eq!(extract_frontmatter_value(c, "foo"), Some("bar"));
        assert_eq!(extract_frontmatter_value(c, "baz"), Some("qux"));
        assert_eq!(extract_frontmatter_value(c, "missing"), None);
    }

    #[test]
    fn extract_value_strips_double_quotes() {
        let c = "---\ntitle: \"hello world\"\n---\nbody\n";
        assert_eq!(extract_frontmatter_value(c, "title"), Some("hello world"));
    }

    #[test]
    fn extract_value_no_frontmatter_returns_none() {
        assert_eq!(extract_frontmatter_value("plain markdown\n", "title"), None);
    }

    // --- Check 3 (freshness) -------------------------------------------------

    fn fm(sot: &str, lv: &str) -> String {
        format!(
            "---\ntitle: t\nmodels: m\nsource_of_truth: {sot}\nlast_verified: {lv}\n---\nbody\n"
        )
    }

    #[test]
    fn freshness_accepts_bootstrap_on_diagram_sot() {
        let oracle = FreshnessOracle::from_set(HashSet::new());
        let content = fm("diagram", "bootstrap 2026-05-23");
        assert!(check_freshness(&content, &oracle).is_ok());
    }

    #[test]
    fn freshness_accepts_bootstrap_on_spec_sot() {
        let oracle = FreshnessOracle::from_set(HashSet::new());
        let content = fm("spec", "bootstrap 2026-05-23");
        assert!(check_freshness(&content, &oracle).is_ok());
    }

    #[test]
    fn freshness_rejects_bootstrap_on_code_sot() {
        let oracle = FreshnessOracle::from_set(HashSet::new());
        let content = fm("code", "bootstrap 2026-05-23");
        let err = check_freshness(&content, &oracle).unwrap_err();
        assert!(err.contains("source_of_truth: code"), "got {err}");
    }

    #[test]
    fn freshness_accepts_sha_in_window() {
        let mut shas = HashSet::new();
        shas.insert("abc1234".to_string());
        let oracle = FreshnessOracle::from_set(shas);
        let content = fm("code", "abc1234 2026-05-23");
        assert!(check_freshness(&content, &oracle).is_ok());
    }

    #[test]
    fn freshness_rejects_sha_outside_window() {
        let mut shas = HashSet::new();
        shas.insert("abc1234".to_string());
        let oracle = FreshnessOracle::from_set(shas);
        let content = fm("code", "deadbee 2026-05-23");
        let err = check_freshness(&content, &oracle).unwrap_err();
        assert!(err.contains("deadbee"), "got {err}");
    }

    #[test]
    fn freshness_rejects_sha_without_date() {
        let oracle = FreshnessOracle::from_set(HashSet::new());
        let content = fm("code", "abc1234");
        assert!(check_freshness(&content, &oracle).is_err());
    }

    #[test]
    fn freshness_rejects_invalid_sot() {
        let oracle = FreshnessOracle::from_set(HashSet::new());
        let content = fm("garbage", "bootstrap 2026-05-23");
        assert!(check_freshness(&content, &oracle).is_err());
    }

    // --- Check 5 (reverse refs) ----------------------------------------------

    fn write_temp(name: &str, body: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("attestrum-test-{nanos}-{name}"));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn scan_pub_items_finds_top_level_only() {
        let src = "\
            //! doc\n\
            pub struct Foo;\n\
            pub fn bar() {}\n\
            pub mod baz;\n\
            fn private() {}\n\
            mod inner {\n  pub struct ShouldNotBeFound;\n}\n\
            pub trait Trait1 {}\n\
            pub enum Color { Red, Blue }\n\
        ";
        let path = write_temp("scan_pub.rs", src);
        let items = scan_pub_items(&path).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"baz"));
        assert!(names.contains(&"Trait1"));
        assert!(names.contains(&"Color"));
        assert!(
            !names.contains(&"ShouldNotBeFound"),
            "nested pub items must be skipped, got {names:?}"
        );
        assert!(!names.contains(&"private"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn scan_pub_items_skips_doc_hidden() {
        let src = "\
            pub struct Visible;\n\
            #[doc(hidden)]\n\
            pub struct Hidden;\n\
            pub fn alsoVisible() {}\n\
        ";
        let path = write_temp("scan_hidden.rs", src);
        let items = scan_pub_items(&path).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Visible"));
        assert!(!names.contains(&"Hidden"));
        assert!(names.contains(&"alsoVisible"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn scan_pub_items_skips_cfg_test() {
        let src = "\
            pub struct Real;\n\
            #[cfg(test)]\n\
            pub mod tests {}\n\
        ";
        let path = write_temp("scan_cfgtest.rs", src);
        let items = scan_pub_items(&path).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Real"));
        assert!(!names.contains(&"tests"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn contains_as_word_matches_exact_token() {
        assert!(contains_as_word(
            "the AttestrumError is bad",
            "AttestrumError"
        ));
        assert!(contains_as_word("AttestrumError\nother", "AttestrumError"));
        assert!(contains_as_word("(AttestrumError)", "AttestrumError"));
    }

    #[test]
    fn contains_as_word_rejects_substring_inside_identifier() {
        assert!(!contains_as_word("AttestrumErrorSpecial", "AttestrumError"));
        assert!(!contains_as_word("Pre_AttestrumError", "AttestrumError"));
        assert!(!contains_as_word("MyAttestrumError", "AttestrumError"));
    }

    #[test]
    fn find_unreferenced_returns_misses() {
        let items = vec![
            PubItem {
                name: "AttestrumError".into(),
                kind: "enum",
                file: PathBuf::from("a.rs"),
            },
            PubItem {
                name: "OrphanType".into(),
                kind: "struct",
                file: PathBuf::from("b.rs"),
            },
        ];
        let blob = "diagram body mentions AttestrumError but not the other";
        let misses = find_unreferenced_pub_items(&items, blob);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].name, "OrphanType");
    }

    // --- Check 4 (forward refs) ----------------------------------------------

    #[test]
    fn extract_models_tokens_splits_on_comma_and_semicolon() {
        let v = "crates/attestrum-core/src/lib.rs, crates/attestrum-signals/src/lib.rs::SignalParser; tools/diagram-linter/src/main.rs";
        let toks = extract_models_tokens(v);
        assert!(toks.iter().any(|t| t == "crates/attestrum-core/src/lib.rs"));
        assert!(toks
            .iter()
            .any(|t| t == "crates/attestrum-signals/src/lib.rs"));
        assert!(toks.iter().any(|t| t == "tools/diagram-linter/src/main.rs"));
    }

    #[test]
    fn extract_models_preserves_bare_identifiers() {
        let v = "AttestrumError, Modality";
        let toks = extract_models_tokens(v);
        assert_eq!(
            toks,
            vec!["AttestrumError".to_string(), "Modality".to_string()]
        );
    }

    #[test]
    fn token_resolves_for_http_url() {
        assert!(token_resolves(
            "https://example.com",
            Path::new("/tmp"),
            &HashSet::new()
        ));
    }

    #[test]
    fn token_resolves_for_attestrum_build_uri() {
        assert!(token_resolves(
            "attestrum.com/attestation/training-corpus/v0.1",
            Path::new("/tmp"),
            &HashSet::new()
        ));
    }

    #[test]
    fn token_resolves_for_bare_id_in_pub_items() {
        let mut pub_items = HashSet::new();
        pub_items.insert("AttestrumError".to_string());
        assert!(token_resolves(
            "AttestrumError",
            Path::new("/tmp"),
            &pub_items
        ));
    }

    #[test]
    fn token_does_not_resolve_for_unknown_bare_id() {
        assert!(!token_resolves(
            "TotallyMadeUp",
            Path::new("/tmp"),
            &HashSet::new()
        ));
    }

    #[test]
    fn token_does_not_resolve_for_missing_file_path() {
        assert!(!token_resolves(
            "definitely/does/not/exist.rs",
            Path::new("/tmp"),
            &HashSet::new()
        ));
    }

    #[test]
    fn token_resolves_for_freeform_phrase() {
        // Things like "Cargo.toml workspace + per-crate manifests" should be accepted.
        assert!(token_resolves(
            "Cargo.toml workspace + per-crate manifests",
            Path::new("/tmp"),
            &HashSet::new()
        ));
    }

    // --- Check 6 (drift) -----------------------------------------------------

    #[test]
    fn drift_detects_code_change_without_diagram_update() {
        let workspace = std::env::temp_dir().join(format!(
            "attestrum-drift-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let diagrams_root = workspace.join("docs/diagrams");
        let code_dir = workspace.join("crates/foo/src");
        std::fs::create_dir_all(&diagrams_root).unwrap();
        std::fs::create_dir_all(&code_dir).unwrap();

        let code_file = code_dir.join("lib.rs");
        std::fs::write(&code_file, "//! foo\n").unwrap();

        let diagram_file = diagrams_root.join("foo.md");
        std::fs::write(
            &diagram_file,
            "---\ntitle: foo\nmodels: crates/foo/src/lib.rs\nsource_of_truth: code\nlast_verified: bootstrap 2026-05-23\n---\n```mermaid\nflowchart TD\n  A --> B\n```\n",
        )
        .unwrap();

        // Code is staged but diagram is NOT — should produce a drift finding.
        let mut changed = HashSet::new();
        changed.insert(code_file.clone());
        let findings = check_drift(&diagrams_root, &workspace, &changed).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code_file, code_file);
        assert_eq!(findings[0].diagram, diagram_file);

        // Now stage the diagram too — drift clears.
        changed.insert(diagram_file.clone());
        let findings2 = check_drift(&diagrams_root, &workspace, &changed).unwrap();
        assert!(
            findings2.is_empty(),
            "drift should clear when both are staged"
        );

        // Cleanup.
        let _ = std::fs::remove_dir_all(&workspace);
    }

    /// Regression test for the 2026-05-25 drift-check path-format bug.
    ///
    /// When the linter is invoked via the CLI with `--root docs/diagrams`
    /// (a relative path), `walk_markdown` returns relative diagram paths,
    /// while `changed_abs` contains absolute paths joined from
    /// `workspace_root`. The contains() check needs to normalize both sides
    /// to absolute or it always reports drift.
    ///
    /// Pre-fix: this test produces a false-positive drift finding even
    /// though both files are staged. Post-fix: drift correctly clears.
    #[test]
    fn drift_clears_when_diagrams_root_is_relative() {
        let workspace = std::env::temp_dir().join(format!(
            "attestrum-drift-rel-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let diagrams_root_abs = workspace.join("docs/diagrams");
        let code_dir = workspace.join("crates/bar/src");
        std::fs::create_dir_all(&diagrams_root_abs).unwrap();
        std::fs::create_dir_all(&code_dir).unwrap();

        let code_file = code_dir.join("lib.rs");
        std::fs::write(&code_file, "//! bar\n").unwrap();

        let diagram_file_abs = diagrams_root_abs.join("bar.md");
        std::fs::write(
            &diagram_file_abs,
            "---\ntitle: bar\nmodels: crates/bar/src/lib.rs\nsource_of_truth: code\nlast_verified: bootstrap 2026-05-23\n---\n```mermaid\nflowchart TD\n  A --> B\n```\n",
        )
        .unwrap();

        // Stage BOTH the code file and the diagram. Production-mode trigger:
        // invoke check_drift with a RELATIVE diagrams_root + absolute changed
        // entries. Without the bug-fix, the comparison fails despite both
        // being staged.
        let cwd_before = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workspace).unwrap();
        let diagrams_root_rel = std::path::PathBuf::from("docs/diagrams");

        let mut changed = HashSet::new();
        changed.insert(code_file.clone()); // absolute
        changed.insert(diagram_file_abs.clone()); // absolute

        let findings = check_drift(&diagrams_root_rel, &workspace, &changed).unwrap();
        // Restore cwd before any assertion can fail.
        std::env::set_current_dir(&cwd_before).unwrap();

        assert!(
            findings.is_empty(),
            "drift should clear when both are staged even with relative diagrams_root; got: {findings:?}"
        );

        // Cleanup.
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn drift_ignores_diagrams_with_non_code_sot() {
        let workspace = std::env::temp_dir().join(format!(
            "attestrum-drift2-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let diagrams_root = workspace.join("docs/diagrams");
        let code_dir = workspace.join("crates/foo/src");
        std::fs::create_dir_all(&diagrams_root).unwrap();
        std::fs::create_dir_all(&code_dir).unwrap();

        let code_file = code_dir.join("lib.rs");
        std::fs::write(&code_file, "//! foo\n").unwrap();
        std::fs::write(
            diagrams_root.join("foo.md"),
            "---\ntitle: foo\nmodels: crates/foo/src/lib.rs\nsource_of_truth: diagram\nlast_verified: bootstrap 2026-05-23\n---\n```mermaid\nflowchart TD\nA-->B\n```\n",
        )
        .unwrap();

        let mut changed = HashSet::new();
        changed.insert(code_file);
        let findings = check_drift(&diagrams_root, &workspace, &changed).unwrap();
        assert!(
            findings.is_empty(),
            "source_of_truth: diagram → drift check skips, got {findings:?}"
        );

        let _ = std::fs::remove_dir_all(&workspace);
    }
}
