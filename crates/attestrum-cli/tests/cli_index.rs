//! Integration tests for `attestrum index build`. Runs the compiled binary via
//! `env!("CARGO_BIN_EXE_attestrum")` (no extra dev-deps), sealing a small corpus
//! with `attestrum build` first, then building the fuzzy sidecars over it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_attestrum")
}

fn fresh_root(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-index-{name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

/// Write a 2-entry text corpus + inputs; return (corpus_toml, workspace).
fn write_corpus(root: &Path) -> (PathBuf, PathBuf) {
    let inputs = root.join("inputs");
    fs::create_dir_all(&inputs).expect("inputs dir");
    fs::write(
        inputs.join("a.txt"),
        "the quick brown fox jumps over the lazy dog while the industrious bee \
         gathers nectar from the blooming wildflowers under a warm afternoon sun",
    )
    .expect("write a");
    fs::write(
        inputs.join("b.txt"),
        "financial regulators published new guidance on capital adequacy ratios \
         for systemically important institutions facing liquidity stress",
    )
    .expect("write b");
    let toml = format!(
        "[corpus]\nname = \"idx-test\"\nsource_date_epoch = 1700000000\n\n\
         [[entry]]\nsource_url = \"{a}/a.txt\"\nmodality = \"text\"\n\n\
         [[entry]]\nsource_url = \"{a}/b.txt\"\nmodality = \"text\"\n",
        a = inputs.display()
    );
    let corpus = root.join("corpus.toml");
    fs::write(&corpus, toml).expect("write corpus.toml");
    (corpus, root.join("ws"))
}

#[test]
fn index_build_happy_path_writes_three_sidecars() {
    let root = fresh_root("happy");
    let (corpus, workspace) = write_corpus(&root);

    let build = Command::new(bin())
        .arg("build")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--source-date-epoch")
        .arg("1700000000")
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let idx = Command::new(bin())
        .args(["index", "build", "--workspace"])
        .arg(&workspace)
        .arg("--source-date-epoch")
        .arg("1700000000")
        .output()
        .expect("run index build");
    assert!(
        idx.status.success(),
        "index build failed: {}",
        String::from_utf8_lossy(&idx.stderr)
    );
    let stdout = String::from_utf8_lossy(&idx.stdout);
    assert!(
        stdout.contains("attestrum index build: ok"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("minhash:     2 leaves"), "stdout: {stdout}");

    for kind in ["minhash", "perceptual", "iscc"] {
        let p = workspace
            .join(".attestrum")
            .join("index")
            .join(kind)
            .join("v1.idx");
        assert!(p.exists(), "missing sidecar: {}", p.display());
    }
}

#[test]
fn index_build_missing_manifest_returns_exit_1() {
    let root = fresh_root("nomanifest");
    let workspace = root.join("ws");
    fs::create_dir_all(&workspace).expect("ws");
    let out = Command::new(bin())
        .args(["index", "build", "--workspace"])
        .arg(&workspace)
        .output()
        .expect("run index build");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("manifest not found"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
