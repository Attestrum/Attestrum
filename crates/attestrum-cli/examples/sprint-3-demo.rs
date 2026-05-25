//! Sprint 3 acceptance demo.
//!
//! End-to-end walkthrough of everything Sprint 3 shipped: the
//! `attestrum-manifest` Parquet writer/reader (E2/E2.5/E3), the
//! Rayon-based `attestrum-pipeline::build_corpus` (E4), and the
//! user-facing CLI subcommands `attestrum build` (E5), `attestrum inspect`
//! (E6), and `attestrum plan` + `attestrum merge` (E7). 100-doc synthetic
//! corpus on disk so the real `attestrum build` code path runs; the
//! subcommand implementations are invoked through the
//! `attestrum_cli::commands::*::run` library API (attestrum-cli is lib+bin
//! since E6) so this demo doesn't need to shell out.
//!
//! **Relative source_urls** in the corpus.toml are used so that
//! `shard_id = BLAKE3(source_url)` from E7 is stable across runs —
//! absolute paths under `env::temp_dir()` would contain a per-process
//! pid+nanos and produce a different shard assignment every run,
//! breaking the cast's reproducibility.
//!
//! Output is captured into `docs/demos/sprint-3.cast` via the
//! checked-in generator at `tools/cast/sprint-3.py` (Python-generated
//! for JSON-escape safety, same pattern as Sprint 1 E12 + Sprint 2
//! E10). Re-run the generator after changing the demo body so the
//! cast stays in sync.

use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use attestrum_cli::commands::{build, inspect, merge, plan};
use attestrum_core::hex;
use attestrum_manifest::read_manifest;
use attestrum_merkle::merkle_root;

const CORPUS_SIZE: u32 = 100;
const SHARDS: u32 = 4;
const PINNED_EPOCH: i64 = 1_748_041_200;

fn main() -> std::io::Result<()> {
    println!("=== Attestrum Sprint 3 acceptance demo ===");
    println!();

    // Per-process scratch root. Paths printed by the CLI subcommands
    // include this dir, so the cast generator substitutes
    // `<pid>-<nanos>` for the actual values at capture time.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "attestrum-sprint-3-demo-{}-{}",
        process::id(),
        nanos
    ));
    fs::create_dir_all(&root)?;

    // -----------------------------------------------------------------
    // Step 1: synthesize the 100-doc corpus on disk. Relative
    // source_urls (`inputs/doc-NNN.txt`) so shard_id is stable.
    // -----------------------------------------------------------------
    println!("--- 100-document synthetic corpus ---");
    let inputs_dir = root.join("inputs");
    fs::create_dir_all(&inputs_dir)?;
    let mut corpus_toml = String::from("[corpus]\nname = \"attestrum-sprint-3-demo\"\n");
    for i in 0..CORPUS_SIZE {
        let body = format!("attestrum-sprint-3-demo doc {i:03}\n");
        let rel = format!("inputs/doc-{i:03}.txt");
        fs::write(inputs_dir.join(format!("doc-{i:03}.txt")), &body)?;
        corpus_toml.push_str("\n[[entry]]\n");
        corpus_toml.push_str(&format!("source_url = \"{rel}\"\n"));
        corpus_toml.push_str("modality = \"text\"\n");
    }
    let corpus_path = root.join("corpus.toml");
    fs::write(&corpus_path, &corpus_toml)?;
    println!(
        "  wrote {CORPUS_SIZE} input files at {}/inputs/",
        root.display()
    );
    println!("  wrote corpus.toml at {}", corpus_path.display());
    println!();

    // -----------------------------------------------------------------
    // Step 2: attestrum build (unsharded). This is the E4 pipeline +
    // E5 CLI subcommand running end-to-end.
    // -----------------------------------------------------------------
    println!("--- E4 + E5: attestrum build (unsharded) ---");
    let ws_unsharded = root.join("ws-unsharded");
    build::run(build::Args {
        corpus: corpus_path.clone(),
        workspace: ws_unsharded.clone(),
        source_date_epoch: Some(PINNED_EPOCH),
        offline: false,
    })
    .expect("attestrum build unsharded");
    let unsharded_manifest = ws_unsharded
        .join(".attestrum")
        .join("manifests")
        .join("manifest.parquet");
    println!();

    // -----------------------------------------------------------------
    // Step 3: attestrum inspect (unsharded).
    // -----------------------------------------------------------------
    println!("--- E6: attestrum inspect (unsharded) ---");
    let code = inspect::run(inspect::Args {
        manifest: unsharded_manifest.clone(),
        offline: false,
    });
    assert_eq!(code, 0, "inspect must exit 0");
    println!();

    // -----------------------------------------------------------------
    // Step 4: attestrum plan --shards 4. Emit shards into the same dir as
    // corpus.toml so the relative `inputs/...` source_urls resolve
    // correctly from each shard.toml's parent dir at build time.
    // -----------------------------------------------------------------
    println!("--- E7: attestrum plan --shards {SHARDS} ---");
    let shards_dir = root.clone();
    let code = plan::run(plan::Args {
        corpus: corpus_path.clone(),
        shards: SHARDS,
        out: shards_dir.clone(),
    });
    assert_eq!(code, 0, "plan must exit 0");
    println!();

    // -----------------------------------------------------------------
    // Step 5: attestrum build each shard (lex-sorted for stable output).
    // -----------------------------------------------------------------
    println!("--- E7: attestrum build (per-shard) ---");
    let mut shard_files: Vec<PathBuf> = fs::read_dir(&shards_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("toml")
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("shard-"))
                    .unwrap_or(false)
        })
        .collect();
    shard_files.sort();
    let mut shard_manifests: Vec<PathBuf> = Vec::with_capacity(shard_files.len());
    for shard in &shard_files {
        let stem = shard
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("shard");
        let ws = root.join(format!("ws-{stem}"));
        println!("  building {stem} -> ws-{stem}/");
        build::run(build::Args {
            corpus: shard.clone(),
            workspace: ws.clone(),
            source_date_epoch: Some(PINNED_EPOCH),
            offline: false,
        })
        .expect("attestrum build shard");
        shard_manifests.push(
            ws.join(".attestrum")
                .join("manifests")
                .join("manifest.parquet"),
        );
    }
    println!();

    // -----------------------------------------------------------------
    // Step 6: attestrum merge.
    // -----------------------------------------------------------------
    println!("--- E7: attestrum merge ---");
    let merged_manifest = root.join("merged.parquet");
    let code = merge::run(merge::Args {
        inputs: shard_manifests.clone(),
        out: merged_manifest.clone(),
    });
    assert_eq!(code, 0, "merge must exit 0");
    println!();

    // -----------------------------------------------------------------
    // Step 7: attestrum inspect the merged manifest.
    // -----------------------------------------------------------------
    println!("--- E6: attestrum inspect (merged) ---");
    let code = inspect::run(inspect::Args {
        manifest: merged_manifest.clone(),
        offline: false,
    });
    assert_eq!(code, 0, "inspect merged must exit 0");
    println!();

    // -----------------------------------------------------------------
    // Step 8: round-trip check — merged root must equal unsharded
    // root. Re-derive both from the on-disk manifests to avoid
    // depending on stdout-parsing.
    // -----------------------------------------------------------------
    println!("--- merge round-trip check ---");
    let rows_un = read_manifest(&unsharded_manifest).expect("read unsharded");
    let rows_merged = read_manifest(&merged_manifest).expect("read merged");
    let mut leaves_un: Vec<[u8; 32]> = rows_un.iter().map(|r| r.document_id).collect();
    leaves_un.sort();
    let mut leaves_merged: Vec<[u8; 32]> = rows_merged.iter().map(|r| r.document_id).collect();
    leaves_merged.sort();
    let root_un = merkle_root(&leaves_un);
    let root_merged = merkle_root(&leaves_merged);
    println!("  unsharded root: {}", hex::encode_32(&root_un));
    println!("  merged    root: {}", hex::encode_32(&root_merged));
    assert_eq!(
        root_un, root_merged,
        "merged root must equal unsharded root"
    );
    println!("  -> MATCH");
    println!();

    // -----------------------------------------------------------------
    // Cleanup + footer
    // -----------------------------------------------------------------
    let _ = fs::remove_dir_all(&root);
    println!("=== SPRINT 3 COMPLETE ===");
    println!("    manifest writer + Rayon pipeline + attestrum build/inspect/plan/merge = green");
    Ok(())
}
