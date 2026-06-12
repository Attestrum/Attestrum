//! Integration tests for `attestrum plan` + `attestrum merge` mapped to the
//! `docs/diagrams/sprint-3/sharding.md` test obligations. Drives the
//! compiled binary via `env!("CARGO_BIN_EXE_attestrum")` + `std::process::Command`
//! — no new dev-deps. Test fixtures live under `CARGO_TARGET_TMPDIR`
//! per the pattern from `crates/attestrum-cas/tests/store.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_manifest::read_manifest;

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-e7-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

fn attestrum_bin() -> &'static str {
    env!("CARGO_BIN_EXE_attestrum")
}

/// Write N inputs at `<root>/inputs/d-NNNN.txt`, each containing
/// "doc body NNNN\n", and a corpus.toml referencing each by absolute
/// path. Returns the path to the corpus.toml.
fn write_unique_corpus(root: &Path, n: u32) -> PathBuf {
    let inputs = root.join("inputs");
    fs::create_dir_all(&inputs).expect("mkdir inputs");
    let mut toml = String::from("[corpus]\nname = \"sharding-test\"\n");
    for i in 0..n {
        let body = format!("doc body {i:04}\n");
        let path = inputs.join(format!("d-{i:04}.txt"));
        fs::write(&path, body).expect("write input");
        toml.push_str("\n[[entry]]\n");
        toml.push_str(&format!("source_url = \"{}\"\n", path.display()));
        toml.push_str("modality = \"text\"\n");
    }
    let corpus = root.join("corpus.toml");
    fs::write(&corpus, toml).expect("write corpus");
    corpus
}

/// Run a subcommand to completion; panic with stderr on non-zero exit.
fn run_attestrum<I, S>(args: I) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let out = Command::new(attestrum_bin())
        .args(args)
        .output()
        .expect("spawn attestrum");
    if !out.status.success() {
        panic!(
            "attestrum exited non-zero ({:?})\nstdout:\n{}\nstderr:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
    out
}

fn list_shard_files(out_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(out_dir)
        .expect("read shards dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    files.sort();
    files
}

/// Number of `[[entry]]` rows in a toml file (cheap text-scan, avoids
/// importing toml::Value just for one count).
fn count_entries(toml_path: &Path) -> usize {
    fs::read_to_string(toml_path)
        .expect("read toml")
        .lines()
        .filter(|l| l.trim() == "[[entry]]")
        .count()
}

// ============================================================================
// Test 1: --shards 1 → one shard with all entries.
// ============================================================================

#[test]
fn plan_shards_1_is_noop_single_shard_file() {
    let root = fresh_root("shards_1");
    let corpus = write_unique_corpus(&root, 7);
    let shards = root.join("shards");
    run_attestrum([
        "plan",
        "--corpus",
        corpus.to_str().unwrap(),
        "--shards",
        "1",
        "--out",
        shards.to_str().unwrap(),
    ]);
    let files = list_shard_files(&shards);
    assert_eq!(files.len(), 1, "shards=1 must emit exactly one shard file");
    assert!(files[0].ends_with("shard-0000.toml"));
    assert_eq!(
        count_entries(&files[0]),
        7,
        "shard-0000 with --shards 1 must contain all input entries"
    );
}

// ============================================================================
// Test 2: distinct source_urls under --shards = entry count.
// Hash collisions are expected for small N; just assert (a) the file count
// is in [1..=N], (b) the entry totals across all shards equal the input,
// and (c) every shard file is non-empty (empty shards are skipped by the
// `plan` emitter).
// ============================================================================

#[test]
fn plan_shards_equal_entry_count_one_per_shard_when_unique_urls() {
    let root = fresh_root("shards_eq_count");
    let n = 16;
    let corpus = write_unique_corpus(&root, n);
    let shards = root.join("shards");
    run_attestrum([
        "plan",
        "--corpus",
        corpus.to_str().unwrap(),
        "--shards",
        &n.to_string(),
        "--out",
        shards.to_str().unwrap(),
    ]);
    let files = list_shard_files(&shards);
    assert!(
        !files.is_empty() && files.len() <= n as usize,
        "expected 1..=N shard files; got {}",
        files.len()
    );
    let total: usize = files.iter().map(|f| count_entries(f)).sum();
    assert_eq!(total, n as usize, "shard entry totals must equal input N");
    for f in &files {
        assert!(
            count_entries(f) > 0,
            "empty shards should be skipped; {} has 0 entries",
            f.display()
        );
    }
}

// ============================================================================
// Test 3: duplicate source_urls always co-locate to the same shard.
// ============================================================================

#[test]
fn plan_duplicate_source_urls_colocate_in_same_shard() {
    let root = fresh_root("dup_urls_colocate");
    let inputs = root.join("inputs");
    fs::create_dir_all(&inputs).expect("mkdir inputs");
    fs::write(inputs.join("shared.txt"), "shared body\n").expect("write");
    let shared_url = inputs.join("shared.txt");

    let mut toml = String::from("[corpus]\nname = \"dup-urls\"\n");
    for _ in 0..5 {
        toml.push_str("\n[[entry]]\n");
        toml.push_str(&format!("source_url = \"{}\"\n", shared_url.display()));
        toml.push_str("modality = \"text\"\n");
    }
    let corpus = root.join("corpus.toml");
    fs::write(&corpus, toml).expect("write corpus");

    let shards = root.join("shards");
    run_attestrum([
        "plan",
        "--corpus",
        corpus.to_str().unwrap(),
        "--shards",
        "8",
        "--out",
        shards.to_str().unwrap(),
    ]);
    let files = list_shard_files(&shards);
    assert_eq!(
        files.len(),
        1,
        "5 entries with identical source_url must co-locate to a single shard file"
    );
    assert_eq!(count_entries(&files[0]), 5);
}

// ============================================================================
// Test 4: re-runs produce byte-identical shard files.
// ============================================================================

#[test]
fn plan_re_run_produces_identical_shard_files() {
    let root_a = fresh_root("rerun_a");
    let root_b = fresh_root("rerun_b");
    let corpus_a = write_unique_corpus(&root_a, 12);
    let corpus_b_inputs = root_b.join("inputs");
    fs::create_dir_all(&corpus_b_inputs).expect("mkdir");

    // Copy inputs and corpus to a fresh root with identical absolute
    // paths inside (the source_url values are absolute paths, so the
    // two roots must have matching path strings inside the toml for
    // byte-identity to be meaningful). Use a different approach:
    // re-run plan twice with the SAME corpus.toml against two
    // different output dirs.
    let _ = corpus_b_inputs;

    let out_a = root_a.join("shards-a");
    let out_b = root_a.join("shards-b");
    run_attestrum([
        "plan",
        "--corpus",
        corpus_a.to_str().unwrap(),
        "--shards",
        "5",
        "--out",
        out_a.to_str().unwrap(),
    ]);
    run_attestrum([
        "plan",
        "--corpus",
        corpus_a.to_str().unwrap(),
        "--shards",
        "5",
        "--out",
        out_b.to_str().unwrap(),
    ]);
    let files_a = list_shard_files(&out_a);
    let files_b = list_shard_files(&out_b);
    assert_eq!(
        files_a.len(),
        files_b.len(),
        "shard file counts differ across runs"
    );
    for (a, b) in files_a.iter().zip(files_b.iter()) {
        assert_eq!(a.file_name(), b.file_name(), "shard file name mismatch");
        let bytes_a = fs::read(a).expect("read a");
        let bytes_b = fs::read(b).expect("read b");
        assert_eq!(
            bytes_a,
            bytes_b,
            "shard {} bytes differ between two runs",
            a.file_name().and_then(|s| s.to_str()).unwrap_or("?")
        );
    }
}

// ============================================================================
// Test 5: round-trip. Build unsharded → root A; plan + build each shard +
// merge → root B; assert root equality AND sorted-leaf-set equality.
//
// Byte-equality of `manifest.parquet` is NOT asserted — diagram body
// documents that merged manifests' `input_ordinal` column reflects
// merge-concat order rather than original input order, so bytes
// generally differ across shards>1. The load-bearing invariant is
// merkle_root equality, which IS asserted.
// ============================================================================

#[test]
fn merge_round_trip_matches_unsharded_build() {
    let root = fresh_root("round_trip");
    let corpus = write_unique_corpus(&root, 9);

    // Unsharded build.
    let ws_un = root.join("ws-unsharded");
    let unsharded_out = run_attestrum([
        "build",
        "--corpus",
        corpus.to_str().unwrap(),
        "--workspace",
        ws_un.to_str().unwrap(),
    ]);
    let stdout_un = String::from_utf8_lossy(&unsharded_out.stdout);
    let root_a = extract_root_from_stdout(&stdout_un);
    let unsharded_manifest = ws_un
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");
    let rows_un = read_manifest(&unsharded_manifest).expect("read unsharded");

    // Plan → build each shard → merge.
    let shards = root.join("shards");
    run_attestrum([
        "plan",
        "--corpus",
        corpus.to_str().unwrap(),
        "--shards",
        "4",
        "--out",
        shards.to_str().unwrap(),
    ]);
    let shard_files = list_shard_files(&shards);
    let mut shard_manifests: Vec<PathBuf> = Vec::new();
    for shard in &shard_files {
        let stem = shard.file_stem().and_then(|s| s.to_str()).unwrap();
        let ws = root.join(format!("ws-{stem}"));
        run_attestrum([
            "build",
            "--corpus",
            shard.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
        ]);
        shard_manifests.push(
            ws.join(".attestrum")
                .join("manifests")
                .join("manifest.parquet"),
        );
    }
    let merged_out = root.join("merged.parquet");
    let mut merge_args: Vec<String> = vec!["merge".into(), "--inputs".into()];
    for m in &shard_manifests {
        merge_args.push(m.to_string_lossy().into_owned());
    }
    merge_args.push("--out".into());
    merge_args.push(merged_out.to_string_lossy().into_owned());
    let merge_result = run_attestrum(merge_args.iter().map(String::as_str));

    // merge must report the canonical root itself: a `merkle_root:` stdout
    // line and a `merkle.root` sibling file, both byte-identical to the
    // unsharded build's (CI consumes these instead of parsing `inspect`).
    let stdout_merge = String::from_utf8_lossy(&merge_result.stdout);
    let root_b = extract_root_from_stdout(&stdout_merge);
    assert_eq!(
        root_a, root_b,
        "merge stdout merkle_root must equal the unsharded build's"
    );
    let root_file_un = ws_un
        .join(".attestrum")
        .join("manifests")
        .join("merkle.root");
    let root_file_merged = root.join("merkle.root");
    assert_eq!(
        fs::read(&root_file_un).expect("read unsharded merkle.root"),
        fs::read(&root_file_merged).expect("read merged merkle.root"),
        "merkle.root sibling file must be byte-identical to the unsharded build's"
    );

    // Inspect both to get the roots. Easier to assert via re-reading
    // and computing merkle_root than parsing stdout.
    let rows_merged = read_manifest(&merged_out).expect("read merged");

    let leaves_un: Vec<[u8; 32]> = {
        let mut v: Vec<[u8; 32]> = rows_un.iter().map(|r| r.document_id).collect();
        v.sort();
        v
    };
    let leaves_merged: Vec<[u8; 32]> = {
        let mut v: Vec<[u8; 32]> = rows_merged.iter().map(|r| r.document_id).collect();
        v.sort();
        v
    };

    assert_eq!(rows_un.len(), 9);
    assert_eq!(rows_merged.len(), 9);
    assert_eq!(
        leaves_un, leaves_merged,
        "sorted leaf set must match between unsharded and merged"
    );

    let root_a_bytes = parse_hex_64(&root_a);
    let merged_root = attestrum_merkle::merkle_root(&leaves_merged);
    assert_eq!(
        root_a_bytes, merged_root,
        "merkle_root must match between unsharded and merged"
    );
}

// ============================================================================
// Test 6: cross-shard overlapping digests get global occurrence_indices.
//
// Bypass `attestrum plan`: hand-build two shards from corpora that have
// distinct source_urls (so they don't co-locate via the deterministic
// shard hash) BUT byte-identical content (so they produce the same
// document_id digest). Merge them and assert the merged manifest has
// two rows with the same document_id and occurrence_index 0 and 1.
// ============================================================================

#[test]
fn merge_with_overlapping_digests_across_shards_globally_reassigns_occurrence_indices() {
    let root = fresh_root("cross_shard_dup");
    let inputs = root.join("inputs");
    fs::create_dir_all(&inputs).expect("mkdir");
    let shared_body = b"shared content across shards\n";
    fs::write(inputs.join("a.txt"), shared_body).expect("write a");
    fs::write(inputs.join("b.txt"), shared_body).expect("write b");

    let mut toml_a = String::from("[corpus]\nname = \"shard-a\"\n\n[[entry]]\n");
    toml_a.push_str(&format!(
        "source_url = \"{}\"\nmodality = \"text\"\n",
        inputs.join("a.txt").display()
    ));
    let corpus_a = root.join("corpus-a.toml");
    fs::write(&corpus_a, toml_a).expect("write a");

    let mut toml_b = String::from("[corpus]\nname = \"shard-b\"\n\n[[entry]]\n");
    toml_b.push_str(&format!(
        "source_url = \"{}\"\nmodality = \"text\"\n",
        inputs.join("b.txt").display()
    ));
    let corpus_b = root.join("corpus-b.toml");
    fs::write(&corpus_b, toml_b).expect("write b");

    let ws_a = root.join("ws-a");
    let ws_b = root.join("ws-b");
    run_attestrum([
        "build",
        "--corpus",
        corpus_a.to_str().unwrap(),
        "--workspace",
        ws_a.to_str().unwrap(),
    ]);
    run_attestrum([
        "build",
        "--corpus",
        corpus_b.to_str().unwrap(),
        "--workspace",
        ws_b.to_str().unwrap(),
    ]);
    let m_a = ws_a
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");
    let m_b = ws_b
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");

    let merged_out = root.join("merged.parquet");
    run_attestrum([
        "merge",
        "--inputs",
        m_a.to_str().unwrap(),
        m_b.to_str().unwrap(),
        "--out",
        merged_out.to_str().unwrap(),
    ]);

    let rows = read_manifest(&merged_out).expect("read merged");
    assert_eq!(rows.len(), 2, "expected two rows in merged manifest");
    let shared_digest = *blake3::hash(shared_body).as_bytes();
    assert_eq!(
        rows[0].document_id, shared_digest,
        "row 0 document_id must be the shared content digest"
    );
    assert_eq!(rows[1].document_id, shared_digest);
    assert_eq!(rows[0].occurrence_index, 0);
    assert_eq!(
        rows[1].occurrence_index, 1,
        "cross-shard duplicate digests must get global occurrence_index 0 and 1, not 0 and 0"
    );
}

// ============================================================================
// Test 7: merkle.root sibling write failure surfaces as exit 1 with the
// RootFile error context (a directory squatting on the sibling path).
// ============================================================================

#[test]
fn merge_merkle_root_sibling_write_failure_exits_nonzero() {
    let root = fresh_root("root_file_err");
    let corpus = write_unique_corpus(&root, 2);
    let ws = root.join("ws");
    run_attestrum([
        "build",
        "--corpus",
        corpus.to_str().unwrap(),
        "--workspace",
        ws.to_str().unwrap(),
    ]);
    let manifest = ws
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");

    // A directory at the sibling path makes the root-file write fail
    // after the merged manifest itself was written successfully.
    let out_dir = root.join("merged");
    fs::create_dir_all(out_dir.join("merkle.root")).expect("mkdir squatter");
    let merged_out = out_dir.join("merged.parquet");

    let out = Command::new(attestrum_bin())
        .args([
            "merge",
            "--inputs",
            manifest.to_str().unwrap(),
            "--out",
            merged_out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn attestrum");
    assert!(
        !out.status.success(),
        "merge must exit non-zero when merkle.root cannot be written"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("merkle.root write failed"),
        "stderr must carry the RootFile context, got:\n{stderr}"
    );
}

// ============================================================================
// helpers
// ============================================================================

fn extract_root_from_stdout(stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix("merkle_root:") {
            return rest.trim().to_string();
        }
    }
    panic!("no merkle_root line in stdout:\n{stdout}");
}

fn parse_hex_64(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64, "hex must be 64 chars, got {}", hex.len());
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let pair = &hex[i * 2..i * 2 + 2];
        *byte = u8::from_str_radix(pair, 16).expect("hex");
    }
    out
}
