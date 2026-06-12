//! Corpus and benchmark ingestion: JSONL and Parquet readers producing a
//! uniform stream of `(id, text)` documents.
//!
//! A missing or non-string text field is a **hard error**, never a silent
//! skip: dropping a document would make a "clean" contamination verdict
//! untrustworthy.

use arrow::array::{Array, LargeStringArray, StringArray, StringViewArray};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// One document to scan, or one benchmark item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    pub id: String,
    pub text: String,
}

/// Failure modes of corpus / benchmark ingestion.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error("unsupported format {ext:?} for {path} (expected .jsonl, .json, or .parquet)")]
    UnsupportedFormat { ext: Option<String>, path: PathBuf },

    #[error("opening {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("reading {path}:{line}: {source}")]
    ReadLine {
        path: PathBuf,
        line: usize,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing JSON at {path}:{line}: {source}")]
    Json {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },

    #[error("missing or non-string field {field:?} at {path}:{line}")]
    MissingTextField {
        field: String,
        path: PathBuf,
        line: usize,
    },

    #[error("reading parquet {path}: {source}")]
    Parquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("column {column:?} not found in {path} (available: {available})")]
    ColumnMissing {
        column: String,
        path: PathBuf,
        available: String,
    },

    #[error("column {column:?} in {path} is not a string column")]
    NotAStringColumn { column: String, path: PathBuf },
}

/// Read a corpus / benchmark file, auto-detecting format by extension.
/// `text_key` names the JSON field or Parquet column holding the text.
pub fn read_corpus(path: &Path, text_key: &str) -> Result<Vec<Doc>, IngestError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("jsonl") | Some("json") => read_jsonl(path, text_key),
        Some("parquet") => read_parquet(path, text_key),
        other => Err(IngestError::UnsupportedFormat {
            ext: other.map(str::to_string),
            path: path.to_path_buf(),
        }),
    }
}

/// Read a JSONL file: one JSON object per line. Blank lines are skipped; lines
/// that fail to parse or lack the text field are hard errors. Document ids: the
/// `id` field if present, else `<file-stem>/<line-index>` (0-based, counting
/// blank lines, matching the enumerate index).
pub fn read_jsonl(path: &Path, text_key: &str) -> Result<Vec<Doc>, IngestError> {
    let file = File::open(path).map_err(|source| IngestError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let stem = file_stem(path);
    let mut docs = Vec::new();
    for (lineno, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|source| IngestError::ReadLine {
            path: path.to_path_buf(),
            line: lineno + 1,
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|source| IngestError::Json {
                path: path.to_path_buf(),
                line: lineno + 1,
                source,
            })?;
        let text = value
            .get(text_key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| IngestError::MissingTextField {
                field: text_key.to_string(),
                path: path.to_path_buf(),
                line: lineno + 1,
            })?
            .to_string();
        let id = match value.get("id") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) if v.is_number() => v.to_string(),
            _ => format!("{stem}/{lineno}"),
        };
        docs.push(Doc { id, text });
    }
    Ok(docs)
}

/// Read a Parquet file's string column `text_column` into docs with ids
/// `<file-stem>/<row-index>`.
pub fn read_parquet(path: &Path, text_column: &str) -> Result<Vec<Doc>, IngestError> {
    let file = File::open(path).map_err(|source| IngestError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let stem = file_stem(path);
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).map_err(|source| IngestError::Parquet {
            path: path.to_path_buf(),
            source,
        })?;
    let reader = builder.build().map_err(|source| IngestError::Parquet {
        path: path.to_path_buf(),
        source,
    })?;
    let mut docs = Vec::new();
    let mut row = 0usize;
    for batch in reader {
        // The Arrow record-batch iterator yields `ArrowError`; fold it into the
        // crate's parquet variant (the `From<ArrowError>` impl exists).
        let batch = batch.map_err(|source| IngestError::Parquet {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        let col = batch
            .column_by_name(text_column)
            .ok_or_else(|| IngestError::ColumnMissing {
                column: text_column.to_string(),
                path: path.to_path_buf(),
                available: batch
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| f.name().as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })?;
        for i in 0..col.len() {
            let text = string_at(col.as_ref(), i).ok_or_else(|| IngestError::NotAStringColumn {
                column: text_column.to_string(),
                path: path.to_path_buf(),
            })?;
            docs.push(Doc {
                id: format!("{stem}/{row}"),
                text,
            });
            row += 1;
        }
    }
    Ok(docs)
}

/// Extract a string value from any of arrow's string array flavors. A null
/// cell reads as the empty string.
fn string_at(array: &dyn Array, i: usize) -> Option<String> {
    if array.is_null(i) {
        return Some(String::new());
    }
    if let Some(a) = array.as_any().downcast_ref::<StringArray>() {
        return Some(a.value(i).to_string());
    }
    if let Some(a) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Some(a.value(i).to_string());
    }
    if let Some(a) = array.as_any().downcast_ref::<StringViewArray>() {
        return Some(a.value(i).to_string());
    }
    None
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("corpus")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("attestrum-decontaminate-ingest-test");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn jsonl_roundtrip_with_and_without_ids() {
        let p = tmp_dir().join("sample.jsonl");
        let mut f = File::create(&p).unwrap();
        writeln!(f, r#"{{"id":"a1","text":"hello world"}}"#).unwrap();
        writeln!(f).unwrap();
        writeln!(f, r#"{{"text":"second doc"}}"#).unwrap();
        let docs = read_jsonl(&p, "text").unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].id, "a1");
        assert_eq!(docs[1].id, "sample/2");
        assert_eq!(docs[1].text, "second doc");
    }

    #[test]
    fn jsonl_numeric_id_is_stringified() {
        let p = tmp_dir().join("numid.jsonl");
        let mut f = File::create(&p).unwrap();
        writeln!(f, r#"{{"id":42,"text":"x"}}"#).unwrap();
        let docs = read_jsonl(&p, "text").unwrap();
        assert_eq!(docs[0].id, "42");
    }

    #[test]
    fn jsonl_missing_field_is_an_error() {
        let p = tmp_dir().join("bad.jsonl");
        let mut f = File::create(&p).unwrap();
        writeln!(f, r#"{{"body":"no text field"}}"#).unwrap();
        assert!(read_jsonl(&p, "text").is_err());
    }

    #[test]
    fn unknown_extension_is_an_error() {
        assert!(read_corpus(Path::new("corpus.csv"), "text").is_err());
    }
}
