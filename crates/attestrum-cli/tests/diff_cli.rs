//! End-to-end test for `attestrum diff`: build two real sealed manifests via the
//! public attestrum-manifest seal epilogue, run the compiled binary, and assert
//! the streaming merge-join path produces the expected delta and exit codes.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("attestrum-diff-cli-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).expect("mkdir tmp");
    dir
}

fn text(id: u8) -> ManifestEntry {
    ManifestEntry {
        document_id: [id; 32],
        sha256: [0u8; 32],
        size_bytes: 100,
        modality: Modality::Text,
        mime_type: None,
        source_url: None,
        source_type: None,
        source_dataset_id: Some("web".into()),
        registered_domain: None,
        license_spdx: None,
        language: Some("en".into()),
        fetched_at: None,
        signals: ManifestSignals::default(),
        included: true,
        exclusion_reason: None,
        chunk_refs: None,
        input_ordinal: 0,
        occurrence_index: 0,
    }
}

fn seal(path: &Path, mut entries: Vec<ManifestEntry>) {
    assign_input_ordinals(&mut entries);
    assign_occurrence_indices(&mut entries);
    sort_entries(&mut entries);
    write_manifest(path, &entries).expect("write manifest");
}

fn attestrum() -> Command {
    Command::new(env!("CARGO_BIN_EXE_attestrum"))
}

#[test]
fn diff_reports_expected_delta_end_to_end() {
    let dir = unique_dir();
    let old_path = dir.join("old.parquet");
    let new_path = dir.join("new.parquet");
    let report = dir.join("report.json");

    seal(&old_path, vec![text(1), text(2), text(3)]); // {1,2,3}
    seal(&new_path, vec![text(1), text(2), text(4)]); // {1,2,4}

    let output = attestrum()
        .arg("diff")
        .arg(&old_path)
        .arg(&new_path)
        .arg("--out")
        .arg(&report)
        .output()
        .expect("run attestrum diff");

    assert!(
        output.status.success(),
        "exit {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let json = std::fs::read_to_string(&report).expect("read report.json");
    assert!(json.contains("\"added\":1"), "report: {json}");
    assert!(json.contains("\"removed\":1"), "report: {json}");
    assert!(json.contains("\"unchanged\":2"), "report: {json}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("corpus diff"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diff_missing_path_exits_2() {
    let dir = unique_dir();
    let old_path = dir.join("old.parquet");
    seal(&old_path, vec![text(1)]);

    let output = attestrum()
        .arg("diff")
        .arg(&old_path)
        .arg(dir.join("does-not-exist.parquet"))
        .output()
        .expect("run attestrum diff");

    assert_eq!(output.status.code(), Some(2), "missing path must exit 2");

    let _ = std::fs::remove_dir_all(&dir);
}
