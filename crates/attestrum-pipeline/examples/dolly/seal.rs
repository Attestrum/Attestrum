//! databricks-dolly-15k Parquet → sealed corpus, the programmatic core of the
//! Tier-1 dolly seal generator (`examples/seal-dolly.rs`).
//!
//! Like its sibling `render.rs`, this file lives under `examples/dolly/` (a
//! *subdirectory*, so Cargo does NOT treat it as an example binary target) and is
//! included two ways, each of which also declares `mod render;` at the same level
//! so the `use crate::render::…` paths below resolve:
//!   - `examples/seal-dolly.rs` (the runnable generator);
//!   - `tests/dolly_seal.rs`, so the `#[cfg(test)]` determinism test below runs
//!     under `cargo test --workspace` (examples are not test-gated by default —
//!     same arrangement as the WikiText seal generator).
//!
//! Reading Parquet needs `arrow` + `parquet`, which are dev-dependencies of this
//! crate. Example and test targets both see dev-deps, so this module compiles in
//! both — but NOT into the library, which stays Parquet-free.
//!
//! **Determinism contract** (extends `sprint-3-corpus`, see the pipeline crate
//! docs): shards are read in sorted filename order, rows in file order; the
//! rendered row text (`render.rs`) is what lands in the CAS. The same shard set
//! sealed twice yields a byte-identical `manifest.parquet` + Merkle root.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use arrow::array::{Array, StringArray};
use attestrum_cas::CasStore;
use attestrum_core::{hex, BuildContext, Modality, SourceType};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildError, BuildOutput, ContentSource, CorpusEntry};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::render::{render, DollyRow};

/// SPDX license of databricks-dolly-15k (CC-BY-SA-3.0; the dataset is partly
/// derived from CC-BY-SA Wikipedia passages, so Databricks releases it under the
/// same ShareAlike terms).
const DOLLY_LICENSE_SPDX: &str = "CC-BY-SA-3.0";
/// Hugging Face dataset id this corpus is sourced from.
const DOLLY_DATASET_ID: &str = "databricks-dolly-15k";

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

    #[error("shard {path} is missing a Utf8 '{column}' column")]
    NoColumn { path: PathBuf, column: &'static str },

    #[error(transparent)]
    Build(#[from] BuildError),
}

/// Downcast a named column of a record batch to a `StringArray`, mapping a
/// missing-or-non-Utf8 column to [`SealError::NoColumn`] carrying the shard path.
fn string_column<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &'static str,
    path: &Path,
) -> Result<&'a StringArray, SealError> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| SealError::NoColumn {
            path: path.to_path_buf(),
            column: name,
        })
}

/// Read the `instruction` / `context` / `response` columns from one Parquet
/// shard, in file order, into [`DollyRow`]s. Null cells become empty strings
/// (`render` drops empty fields). The `category` column is intentionally not
/// read — it is not part of the sealed leaf.
fn read_rows(path: &Path) -> Result<Vec<DollyRow>, SealError> {
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
        let batch = batch.map_err(|source| SealError::Parquet {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        let instruction = string_column(&batch, "instruction", path)?;
        let context = string_column(&batch, "context", path)?;
        let response = string_column(&batch, "response", path)?;
        let cell = |a: &StringArray, i: usize| -> String {
            if a.is_null(i) {
                String::new()
            } else {
                a.value(i).to_string()
            }
        };
        for i in 0..batch.num_rows() {
            rows.push(DollyRow {
                instruction: cell(instruction, i),
                context: cell(context, i),
                response: cell(response, i),
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

/// Read every shard in `dir` into rows, in (shard filename, row) order.
pub fn rows_from_dir(dir: &Path) -> Result<Vec<DollyRow>, SealError> {
    let mut rows: Vec<DollyRow> = Vec::new();
    for shard in sorted_shards(dir)? {
        rows.extend(read_rows(&shard)?);
    }
    Ok(rows)
}

/// Map dolly rows to pipeline corpus entries. Each row is one leaf; the sealed
/// bytes are the rendered natural text (`render.rs`). The `source_uri` backref is
/// `dolly-15k://train#row<N>` (0-based, in file order). Provenance metadata is
/// fixed for the public databricks-dolly-15k dataset.
pub fn rows_to_entries(rows: Vec<DollyRow>) -> Vec<CorpusEntry> {
    rows.into_iter()
        .enumerate()
        .map(|(n, r)| CorpusEntry {
            source_uri: format!("dolly-15k://train#row{n}"),
            content: ContentSource::Bytes(render(&r).into_bytes()),
            modality: Modality::Text,
            mime_type: Some("text/plain".to_string()),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some(DOLLY_DATASET_ID.to_string()),
            registered_domain: None,
            license_spdx: Some(DOLLY_LICENSE_SPDX.to_string()),
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
/// it, but it is part of the deterministic build context). Mirrors the WikiText
/// seal so its output is a drop-in for the publish path's `--merkle-root` arg.
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

    // Write `merkle.root` as a sibling to manifest.parquet — 64 lowercase hex
    // chars + trailing newline, byte-identical to what `attestrum build` emits.
    // This is the file the publish path's `--merkle-root` argument points at. It
    // is an additional sibling: it does not affect manifest.parquet or the Merkle
    // root, so the stdout root stays the source of truth for the seal-crosscheck.
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

    /// Write a four-column (`instruction`/`context`/`response`/`category`, all
    /// Utf8) Parquet shard — the real dolly schema, including the `category`
    /// column the seal deliberately ignores.
    fn write_shard(path: &Path, rows: &[(&str, &str, &str, &str)]) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("instruction", DataType::Utf8, false),
            Field::new("context", DataType::Utf8, false),
            Field::new("response", DataType::Utf8, false),
            Field::new("category", DataType::Utf8, false),
        ]));
        let instruction = StringArray::from(rows.iter().map(|r| r.0).collect::<Vec<_>>());
        let context = StringArray::from(rows.iter().map(|r| r.1).collect::<Vec<_>>());
        let response = StringArray::from(rows.iter().map(|r| r.2).collect::<Vec<_>>());
        let category = StringArray::from(rows.iter().map(|r| r.3).collect::<Vec<_>>());
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(instruction),
                Arc::new(context),
                Arc::new(response),
                Arc::new(category),
            ],
        )
        .unwrap();
        let file = File::create(path).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
    }

    fn fixture_rows() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        vec![
            (
                "When did Virgin Australia start operating?",
                "Virgin Australia commenced services on 31 August 2000.",
                "It started operating on 31 August 2000.",
                "closed_qa",
            ),
            // Empty context — the common dolly shape.
            (
                "Why is the sky blue?",
                "",
                "Rayleigh scattering.",
                "open_qa",
            ),
            (
                "Give three primary colors.",
                "",
                "Red, green, and blue.",
                "brainstorming",
            ),
        ]
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("attestrum-dolly-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_rows_and_maps_fixed_provenance() {
        let input = unique_dir("rows-input");
        write_shard(&input.join("0000.parquet"), &fixture_rows());

        let rows = rows_from_dir(&input).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].context, ""); // category column ignored; context preserved

        let entries = rows_to_entries(rows);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].source_uri, "dolly-15k://train#row0");
        assert_eq!(entries[2].source_uri, "dolly-15k://train#row2");
        assert!(entries.iter().all(|e| e.modality == Modality::Text));
        assert!(entries
            .iter()
            .all(|e| e.source_dataset_id.as_deref() == Some("databricks-dolly-15k")));
        assert!(entries
            .iter()
            .all(|e| e.license_spdx.as_deref() == Some("CC-BY-SA-3.0")));
        // The empty-context row renders with no blank-line gap and no category.
        if let ContentSource::Bytes(b) = &entries[1].content {
            assert_eq!(b, b"Why is the sky blue?\n\nRayleigh scattering.\n");
        } else {
            panic!("expected Bytes content");
        }

        let _ = fs::remove_dir_all(&input);
    }

    #[test]
    fn seal_is_deterministic_over_fixture_parquet() {
        let input = unique_dir("det-input");
        write_shard(&input.join("0000.parquet"), &fixture_rows());

        let entries = rows_to_entries(rows_from_dir(&input).unwrap());
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

        // merkle.root sibling: 64 lowercase hex + newline, byte-identical across runs.
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
