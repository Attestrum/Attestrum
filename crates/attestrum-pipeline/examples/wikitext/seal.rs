//! WikiText-103 Parquet → sealed corpus, the programmatic core of the Lookback
//! seal generator (`examples/seal-wikitext.rs`).
//!
//! Like its sibling `segment.rs`, this file lives under `examples/wikitext/` (a
//! *subdirectory*, so Cargo does NOT treat it as an example binary target) and is
//! included two ways, each of which also declares `mod segment;` at the same
//! level so the `use crate::segment::…` paths below resolve:
//!   - `examples/seal-wikitext.rs` (the runnable generator);
//!   - `tests/wikitext_seal.rs`, so the `#[cfg(test)]` determinism test below
//!     runs under `cargo test --workspace` (examples are not test-gated by
//!     default — same arrangement as `sprint-3-corpus` / `cross_platform_inputs`).
//!
//! Reading Parquet needs `arrow` + `parquet`, which are dev-dependencies of this
//! crate. Example and test targets both see dev-deps, so this module compiles in
//! both — but NOT into the library, which stays Parquet-free.
//!
//! **Determinism contract** (extends `sprint-3-corpus`, see the pipeline crate
//! docs): shards are read in sorted filename order, rows in file order; the
//! detokenized passage text is what lands in the CAS. The same shard set sealed
//! twice yields a byte-identical `manifest.parquet` + Merkle root.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use arrow::array::{Array, StringArray};
use attestrum_cas::CasStore;
use attestrum_core::{hex, BuildContext, Modality, SourceType};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildError, BuildOutput, ContentSource, CorpusEntry};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::segment::{segment as segment_lines, Passage};

/// SPDX license of WikiText-103 (Wikipedia text, CC-BY-SA-3.0).
const WIKITEXT_LICENSE_SPDX: &str = "CC-BY-SA-3.0";
/// Hugging Face dataset id this corpus is sourced from.
const WIKITEXT_DATASET_ID: &str = "wikitext-103-raw-v1";

/// Errors the seal generator can surface. The protected pipeline error
/// (`BuildError`) flows through `Build`; everything upstream of `build_corpus`
/// (Parquet I/O, schema shape) gets its own variant carrying the offending path.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("parquet error reading {path}: {source}")]
    Parquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("shard {path} has no Utf8 'text' column")]
    NoTextColumn { path: PathBuf },

    #[error(transparent)]
    Build(#[from] BuildError),
}

/// Read every value of the Utf8 `text` column from one Parquet shard, in file
/// order. Null rows become empty strings (segmentation skips empty lines).
fn read_text_rows(path: &Path) -> Result<Vec<String>, SealError> {
    let file = File::open(path).map_err(|source| SealError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .and_then(|b| b.build())
        .map_err(|source| SealError::Parquet {
            path: path.to_path_buf(),
            source,
        })?;

    let mut rows = Vec::new();
    for batch in reader {
        // The record-batch iterator yields ArrowError; fold it into the
        // Parquet variant (ArrowError: Into<ParquetError>).
        let batch = batch.map_err(|source| SealError::Parquet {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        let col = batch
            .column_by_name("text")
            .ok_or_else(|| SealError::NoTextColumn {
                path: path.to_path_buf(),
            })?;
        let text =
            col.as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| SealError::NoTextColumn {
                    path: path.to_path_buf(),
                })?;
        for i in 0..text.len() {
            rows.push(if text.is_null(i) {
                String::new()
            } else {
                text.value(i).to_string()
            });
        }
    }
    Ok(rows)
}

/// List the `*.parquet` shards in `dir`, sorted by filename (the deterministic
/// shard order the manifest's input ordinals derive from).
fn sorted_shards(dir: &Path) -> Result<Vec<PathBuf>, SealError> {
    let mut shards: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|source| SealError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| SealError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
            shards.push(path);
        }
    }
    shards.sort();
    Ok(shards)
}

/// Segment all shards in `dir` into detokenized passages.
///
/// Rows from every shard are concatenated in (shard filename, row) order and fed
/// to [`segment`](crate::segment::segment) as one document, so the article-title
/// state machine spans row and shard boundaries correctly (a `= Title =` line in
/// one shard governs the body lines that follow it in the next).
pub fn passages_from_dir(dir: &Path) -> Result<Vec<Passage>, SealError> {
    let mut lines: Vec<String> = Vec::new();
    for shard in sorted_shards(dir)? {
        lines.extend(read_text_rows(&shard)?);
    }
    Ok(segment_lines(&lines.join("\n")))
}

/// Map detokenized passages to pipeline corpus entries. Each passage is one leaf;
/// the sealed bytes are the detokenized natural-English text. Provenance metadata
/// is fixed for the WikiText-103 public dataset.
pub fn passages_to_entries(passages: Vec<Passage>) -> Vec<CorpusEntry> {
    passages
        .into_iter()
        .map(|p| CorpusEntry {
            source_uri: p.source_uri,
            content: ContentSource::Bytes(p.text.into_bytes()),
            modality: Modality::Text,
            mime_type: Some("text/plain".to_string()),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some(WIKITEXT_DATASET_ID.to_string()),
            registered_domain: None,
            license_spdx: Some(WIKITEXT_LICENSE_SPDX.to_string()),
            language: Some("en".to_string()),
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
        })
        .collect()
}

/// Seal `entries` into `output_dir`: a CAS under `<output_dir>/.attestrum/` and a
/// `manifest.parquet` under `<output_dir>/.attestrum/manifests/`. `epoch` is the
/// `--source-date-epoch` value (0 for the demo; the pipeline does not yet inject
/// it, but it is part of the deterministic build context).
pub fn seal(
    entries: &[CorpusEntry],
    output_dir: &Path,
    epoch: i64,
) -> Result<BuildOutput, SealError> {
    let cas = CasStore::new(output_dir.join(".attestrum")).map_err(|source| SealError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;
    let ctx = BuildContext::new(output_dir.to_path_buf(), epoch);
    let manifest_dir = output_dir.join(".attestrum").join("manifests");
    let output = build_corpus(&ctx, &cas, entries, &manifest_dir)?;

    // Write `merkle.root` as a sibling to manifest.parquet — 64 lowercase hex chars
    // + trailing newline, byte-identical to what `attestrum build` emits
    // (`attestrum-cli/src/commands/build.rs`). This is the file the publish path's
    // `--merkle-root` argument points at; sealing here keeps the example's output a
    // drop-in for that path. It is an additional sibling file: it does not affect
    // manifest.parquet or the Merkle root, so the stdout root stays the source of
    // truth for the seal-crosscheck gate.
    let merkle_root_path = manifest_dir.join("merkle.root");
    fs::write(
        &merkle_root_path,
        format!("{}\n", hex::encode_32(&output.merkle_root)),
    )
    .map_err(|source| SealError::Io {
        path: merkle_root_path,
        source,
    })?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;

    /// Write a single-column (`text: Utf8`) Parquet shard with the given lines.
    fn write_shard(path: &Path, lines: &[&str]) {
        let schema = Arc::new(Schema::new(vec![Field::new("text", DataType::Utf8, false)]));
        let array = StringArray::from(lines.to_vec());
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(array)]).unwrap();
        let file = File::create(path).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
    }

    /// A two-article fixture exercising titles, sections, joiners, punctuation,
    /// and the min-word floor — split across two shards so the cross-shard
    /// title→body carry is covered.
    fn fixture_shard_a() -> Vec<&'static str> {
        vec![
            " = Valkyria Chronicles III = ",
            "",
            " Senjo no Valkyria 3 is a tactical role @-@ playing video game developed by Sega .",
            "",
            " = = Gameplay = = ",
            "",
            " The game is a tactical role @-@ playing game where the player controls a squad .",
            " short",
        ]
    }

    fn fixture_shard_b() -> Vec<&'static str> {
        vec![
            " = Tower Building = ",
            "",
            " The tower was 1 @,@ 000 feet tall and it was n't the tallest in the city .",
        ]
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("attestrum-seal-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn segments_across_shards_with_fixed_provenance() {
        let input = unique_dir("seg-input");
        write_shard(&input.join("train-00000.parquet"), &fixture_shard_a());
        write_shard(&input.join("train-00001.parquet"), &fixture_shard_b());

        let passages = passages_from_dir(&input).unwrap();
        // 3 body lines clear the 5-word floor (" short" is dropped); section
        // heading is skipped; both article titles set their slug.
        assert_eq!(passages.len(), 3);
        assert_eq!(
            passages[0].source_uri,
            "wikipedia://Valkyria_Chronicles_III#p1"
        );
        assert_eq!(
            passages[1].source_uri,
            "wikipedia://Valkyria_Chronicles_III#p2"
        );
        // The second article's title governs the body line in the *next* shard.
        assert_eq!(passages[2].source_uri, "wikipedia://Tower_Building#p1");
        assert!(passages[0].text.contains("role-playing"));
        assert!(passages[2].text.contains("1,000 feet"));
        assert!(passages[2].text.contains("wasn't"));

        let entries = passages_to_entries(passages);
        assert!(entries.iter().all(|e| e.modality == Modality::Text));
        assert!(entries
            .iter()
            .all(|e| e.source_dataset_id.as_deref() == Some("wikitext-103-raw-v1")));
        assert!(entries
            .iter()
            .all(|e| e.license_spdx.as_deref() == Some("CC-BY-SA-3.0")));

        let _ = fs::remove_dir_all(&input);
    }

    #[test]
    fn seal_is_deterministic_over_fixture_parquet() {
        let input = unique_dir("det-input");
        write_shard(&input.join("train-00000.parquet"), &fixture_shard_a());
        write_shard(&input.join("train-00001.parquet"), &fixture_shard_b());

        let entries = passages_to_entries(passages_from_dir(&input).unwrap());
        assert!(!entries.is_empty());

        let ws1 = unique_dir("det-ws1");
        let ws2 = unique_dir("det-ws2");
        let out1 = seal(&entries, &ws1, 0).unwrap();
        let out2 = seal(&entries, &ws2, 0).unwrap();

        assert_eq!(out1.merkle_root, out2.merkle_root, "Merkle root must match");
        assert_eq!(out1.leaf_count, entries.len());
        assert_eq!(
            fs::read(&out1.manifest_path).unwrap(),
            fs::read(&out2.manifest_path).unwrap(),
            "manifest.parquet bytes must be identical"
        );

        // merkle.root sibling: written beside manifest.parquet, formatted as 64
        // lowercase hex + newline (matching `attestrum build`), and byte-identical
        // across the two deterministic runs.
        let root_file_1 = ws1.join(".attestrum").join("manifests").join("merkle.root");
        let root_file_2 = ws2.join(".attestrum").join("manifests").join("merkle.root");
        let root_text_1 = fs::read_to_string(&root_file_1).unwrap();
        assert_eq!(
            root_text_1,
            format!("{}\n", hex::encode_32(&out1.merkle_root)),
            "merkle.root must be 64 lowercase hex + newline matching the manifest root"
        );
        assert_eq!(
            root_text_1,
            fs::read_to_string(&root_file_2).unwrap(),
            "merkle.root must be byte-identical across deterministic runs"
        );

        for d in [&input, &ws1, &ws2] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
