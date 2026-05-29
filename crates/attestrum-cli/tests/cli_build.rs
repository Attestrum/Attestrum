//! Integration tests for `attestrum build`, mapped to the
//! `docs/diagrams/sprint-3/attestrum-build-cli.md` test obligations. Runs
//! the compiled binary via `env!("CARGO_BIN_EXE_attestrum")` — no extra
//! dev-deps. Test fixtures live under `CARGO_TARGET_TMPDIR` per the
//! pattern from `crates/attestrum-cas/tests/store.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_manifest::read_manifest;

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-e5-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

/// Path to the compiled `attestrum` binary that cargo built for this test
/// crate. `CARGO_BIN_EXE_<name>` is set by cargo when the test crate
/// depends on a [[bin]] target.
fn attestrum_bin() -> &'static str {
    env!("CARGO_BIN_EXE_attestrum")
}

/// Write the 3-entry test corpus + content files, return (corpus_toml_path, workspace_dir).
fn write_corpus_and_inputs(root: &std::path::Path, epoch: Option<i64>) -> (PathBuf, PathBuf) {
    let inputs = root.join("inputs");
    fs::create_dir_all(&inputs).expect("create inputs dir");
    for i in 0..3u32 {
        let body = format!("happy-path doc {i}\n");
        fs::write(inputs.join(format!("d-{i}.txt")), body).expect("write input");
    }
    let mut toml = String::new();
    toml.push_str("[corpus]\n");
    toml.push_str("name = \"happy-path-3\"\n");
    if let Some(e) = epoch {
        toml.push_str(&format!("source_date_epoch = {e}\n"));
    }
    for i in 0..3u32 {
        toml.push_str("\n[[entry]]\n");
        toml.push_str(&format!(
            "source_url = \"{}/d-{i}.txt\"\n",
            inputs.display()
        ));
        toml.push_str("modality = \"text\"\n");
        toml.push_str("mime_type = \"text/plain\"\n");
        toml.push_str("source_type = \"public_dataset\"\n");
        toml.push_str("license_spdx = \"CC0-1.0\"\n");
    }
    let corpus = root.join("corpus.toml");
    fs::write(&corpus, toml).expect("write corpus.toml");
    let workspace = root.join("workspace");
    (corpus, workspace)
}

#[test]
fn build_happy_path_returns_exit_0_with_summary() {
    let root = fresh_root("happy_path");
    let (corpus, workspace) = write_corpus_and_inputs(&root, Some(1_700_000_000));

    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&workspace)
        .output()
        .expect("spawn attestrum build");

    assert!(
        out.status.success(),
        "expected exit 0; got {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("attestrum build: ok"),
        "stdout was:\n{stdout}"
    );
    assert!(
        stdout.contains("merkle_root:"),
        "stdout missing merkle_root line:\n{stdout}"
    );
    assert!(
        stdout.contains("leaf_count:   3"),
        "stdout missing leaf_count line:\n{stdout}"
    );
    let manifest = workspace
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");
    assert!(
        manifest.exists(),
        "expected manifest at {}",
        manifest.display()
    );

    // The Merkle-root sidecar must land next to manifest.parquet so
    // `attestrum publish` can commit it as `attestrum/merkle.root` per
    // docs/diagrams/overview/hub-publish.md. Format contract: 64 lowercase
    // hex chars + trailing newline = exactly 65 bytes. The hex value must
    // also match the merkle_root line in the stdout summary.
    let merkle_root_file = workspace
        .join(".attestrum")
        .join("manifests")
        .join("merkle.root");
    assert!(
        merkle_root_file.exists(),
        "expected merkle.root sidecar at {}",
        merkle_root_file.display()
    );
    let merkle_root_bytes = fs::read(&merkle_root_file).expect("read merkle.root");
    assert_eq!(
        merkle_root_bytes.len(),
        65,
        "merkle.root must be 64 hex chars + newline (got {} bytes)",
        merkle_root_bytes.len()
    );
    let merkle_root_str = String::from_utf8(merkle_root_bytes).expect("merkle.root is utf-8");
    assert!(
        merkle_root_str.ends_with('\n'),
        "merkle.root must end with newline"
    );
    let hex_only = merkle_root_str.trim_end_matches('\n');
    assert_eq!(
        hex_only.len(),
        64,
        "trimmed hex must be exactly 64 chars (got {})",
        hex_only.len()
    );
    assert!(
        hex_only
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "merkle.root must be lowercase hex; got {hex_only:?}"
    );
    assert!(
        stdout.contains(hex_only),
        "stdout merkle_root line must match merkle.root file contents (hex={hex_only:?}, stdout:\n{stdout})"
    );
}

#[test]
fn missing_corpus_file_returns_exit_1() {
    let root = fresh_root("missing_corpus");
    let corpus = root.join("nope.toml");
    let workspace = root.join("workspace");
    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&workspace)
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, got {:?}",
        out.status.code()
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("corpus file not found") && stderr.contains("nope.toml"),
        "stderr did not mention missing file:\n{stderr}"
    );
}

#[test]
fn malformed_corpus_toml_returns_exit_1() {
    let root = fresh_root("malformed_toml");
    let corpus = root.join("bad.toml");
    fs::write(&corpus, "[corpus\nthis is not valid toml at all").expect("write bad toml");
    let workspace = root.join("workspace");
    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&workspace)
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, got {:?}",
        out.status.code()
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("corpus.toml parse failed"),
        "stderr did not mention parse error:\n{stderr}"
    );
}

#[test]
fn clap_arg_parse_failure_returns_exit_2() {
    // No --corpus; clap-native arg-parse error.
    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--workspace")
        .arg("/tmp/whatever")
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 (clap), got {:?}",
        out.status.code()
    );
}

#[test]
fn source_date_epoch_is_plumbed_into_manifest() {
    let root = fresh_root("sde_plumb");
    let (corpus, workspace) = write_corpus_and_inputs(&root, None);
    let epoch = 1_700_000_000_i64;
    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--source-date-epoch")
        .arg(epoch.to_string())
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "expected exit 0; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = workspace
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");
    let rows = read_manifest(&manifest).expect("read manifest");
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(
            row.fetched_at,
            Some(epoch),
            "expected --source-date-epoch to plumb through to per-entry fetched_at when corpus.toml omits it"
        );
    }
}

#[test]
fn workspace_directory_is_created_if_missing() {
    let root = fresh_root("workspace_mkdir");
    let (corpus, _unused_workspace) = write_corpus_and_inputs(&root, Some(0));
    let nested = root.join("a").join("b").join("workspace-fresh");
    assert!(!nested.exists());

    let out = Command::new(attestrum_bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&nested)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "expected exit 0; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(nested.is_dir());
    assert!(nested
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet")
        .exists());
}
