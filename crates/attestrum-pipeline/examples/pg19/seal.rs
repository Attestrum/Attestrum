//! deepmind-pg19 file tree → sealed corpus, the programmatic core of the
//! Tier-1 PG-19 seal generator (`examples/seal-pg19.rs`).
//!
//! Like its dolly/wikitext siblings, this file lives under `examples/pg19/` (a
//! *subdirectory*, so Cargo does NOT treat it as an example binary target) and
//! is included two ways:
//!   - `examples/seal-pg19.rs` (the runnable generator);
//!   - `tests/pg19_seal.rs`, so the `#[cfg(test)]` determinism test below runs
//!     under `cargo test --workspace`.
//!
//! Unlike dolly there is no Parquet and no render step: PG-19 ships as one
//! plain-text file per book (`train/` 28,602, `validation/` 50, `test/` 100),
//! and **one file = one leaf, sealed as its exact bytes** — the contract pinned
//! by `docs/diagrams/lookback/pg19-seal-pipeline.md`. Entries carry
//! `ContentSource::Path`, so the ~11.5 GB corpus is never resident in memory:
//! `build_corpus` reads one file per worker at a time (max file ~4.5 MB).
//!
//! **Determinism contract** (extends `sprint-3-corpus`): files are enumerated
//! from the three split subdirs and sorted lexicographically by relative path;
//! `build_corpus` stamps `input_ordinal` then sorts canonically. The same file
//! tree sealed twice yields a byte-identical `manifest.parquet` + Merkle root.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use attestrum_cas::CasStore;
use attestrum_core::{hex, BuildContext, Modality, SourceType};
use attestrum_manifest::ManifestSignals;
use attestrum_pipeline::{build_corpus, BuildError, BuildOutput, ContentSource, CorpusEntry};

/// SPDX license of the PG-19 dataset compilation (DeepMind). The underlying
/// book texts are pre-1919 Project Gutenberg public domain.
const PG19_LICENSE_SPDX: &str = "Apache-2.0";
/// Hugging Face dataset id this corpus is sourced from.
const PG19_DATASET_ID: &str = "deepmind-pg19";
/// The dataset's three split directories, in the fixed order they are walked.
/// (Order does not affect the seal — entries are sorted globally by relative
/// path afterwards — but a fixed walk order keeps error output stable.)
const SPLITS: [&str; 3] = ["train", "validation", "test"];

/// Errors the seal generator can surface. The protected pipeline error
/// (`BuildError`) flows through `Build`; everything upstream of `build_corpus`
/// (directory walking) gets its own variant carrying the offending path.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("input dir {0} has none of the split subdirs train/, validation/, test/")]
    NoSplits(PathBuf),

    #[error("no .txt book files found under {0}")]
    Empty(PathBuf),

    #[error(transparent)]
    Build(#[from] BuildError),
}

/// Enumerate the `.txt` book files under `<input_dir>/{train,validation,test}/`,
/// returned as relative paths (e.g. `train/10005.txt`) sorted lexicographically
/// — the deterministic order the manifest's input ordinals derive from. Split
/// dirs may be individually absent (a fixture or smoke corpus need not carry all
/// three), but at least one must exist and at least one book must be found.
pub fn book_paths(input_dir: &Path) -> Result<Vec<PathBuf>, SealError> {
    let mut saw_split = false;
    let mut books: Vec<PathBuf> = Vec::new();
    for split in SPLITS {
        let dir = input_dir.join(split);
        if !dir.is_dir() {
            continue;
        }
        saw_split = true;
        for entry in fs::read_dir(&dir).map_err(|source| SealError::Io {
            path: dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| SealError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("txt") {
                books.push(PathBuf::from(split).join(entry.file_name()));
            }
        }
    }
    if !saw_split {
        return Err(SealError::NoSplits(input_dir.to_path_buf()));
    }
    if books.is_empty() {
        return Err(SealError::Empty(input_dir.to_path_buf()));
    }
    books.sort();
    Ok(books)
}

/// Map book files to pipeline corpus entries. Each file is one leaf; the sealed
/// bytes are the file's exact bytes (`ContentSource::Path` — `build_corpus`
/// reads them, this function never does). The `source_uri` backref is
/// `pg19://<relative-path>` (e.g. `pg19://train/10005.txt`). Provenance
/// metadata is fixed for the public deepmind-pg19 dataset.
pub fn paths_to_entries(input_dir: &Path, books: Vec<PathBuf>) -> Vec<CorpusEntry> {
    books
        .into_iter()
        .map(|rel| CorpusEntry {
            // Relative paths are ASCII `<split>/<digits>.txt`, so display() is
            // lossless and identical across platforms (`/` join on both).
            source_uri: format!("pg19://{}", rel.display()),
            content: ContentSource::Path(input_dir.join(&rel)),
            modality: Modality::Text,
            mime_type: Some("text/plain".to_string()),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some(PG19_DATASET_ID.to_string()),
            registered_domain: None,
            license_spdx: Some(PG19_LICENSE_SPDX.to_string()),
            language: Some("en".to_string()),
            fetched_at: None,
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
        })
        .collect()
}

/// Seal `entries` into `output_dir`: a CAS under `<output_dir>/.attestrum/` and
/// a `manifest.parquet` under `<output_dir>/.attestrum/manifests/`. `epoch` is
/// the `--source-date-epoch` value (0 for the demo). Mirrors the dolly/WikiText
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
    // This is the file the publish path's `--merkle-root` argument points at.
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

    /// Lay down a fixture PG-19-shaped tree: a few small "books" across the
    /// three split dirs, plus a non-.txt file the walker must ignore.
    fn write_fixture_tree(root: &Path) {
        for (rel, body) in [
            ("train/10.txt", "Call me Ishmael. Some years ago...\n"),
            ("train/2.txt", "It was the best of times.\n"),
            ("validation/7.txt", "In the beginning was the word.\n"),
            ("test/99.txt", "All happy families are alike.\n"),
        ] {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
        }
        fs::write(root.join("train/ignore-me.csv"), "not,a,book\n").unwrap();
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("attestrum-pg19-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn walks_books_sorted_and_maps_fixed_provenance() {
        let input = unique_dir("walk-input");
        write_fixture_tree(&input);

        let books = book_paths(&input).unwrap();
        // Lexicographic by relative path: numeric ids sort as strings ("10" < "2").
        assert_eq!(
            books,
            vec![
                PathBuf::from("test/99.txt"),
                PathBuf::from("train/10.txt"),
                PathBuf::from("train/2.txt"),
                PathBuf::from("validation/7.txt"),
            ]
        );

        let entries = paths_to_entries(&input, books);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].source_uri, "pg19://test/99.txt");
        assert_eq!(entries[2].source_uri, "pg19://train/2.txt");
        assert!(entries.iter().all(|e| e.modality == Modality::Text));
        assert!(entries
            .iter()
            .all(|e| e.source_dataset_id.as_deref() == Some("deepmind-pg19")));
        assert!(entries
            .iter()
            .all(|e| e.license_spdx.as_deref() == Some("Apache-2.0")));
        // Exact-bytes contract: the entry points at the file, no Bytes copy.
        assert!(entries
            .iter()
            .all(|e| matches!(&e.content, ContentSource::Path(p) if p.is_file())));

        let _ = fs::remove_dir_all(&input);
    }

    #[test]
    fn missing_splits_and_empty_corpus_are_errors() {
        let no_splits = unique_dir("err-no-splits");
        assert!(matches!(
            book_paths(&no_splits),
            Err(SealError::NoSplits(_))
        ));

        let empty = unique_dir("err-empty");
        fs::create_dir_all(empty.join("train")).unwrap();
        assert!(matches!(book_paths(&empty), Err(SealError::Empty(_))));

        for d in [&no_splits, &empty] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn unreadable_split_dir_is_an_io_error() {
        // An unreadable split directory must surface as SealError::Io (not be
        // silently skipped). chmod 000 only blocks non-root processes — the
        // determinism matrix's musl job runs in a container as root, where the
        // kernel ignores permission bits — so detect euid via the uid stamped
        // on a file this process creates (std-only) and skip under root.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::PermissionsExt;

            let input = unique_dir("err-unreadable");
            let dir = input.join("train");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("1.txt"), "x\n").unwrap();
            if fs::metadata(dir.join("1.txt")).unwrap().uid() == 0 {
                // Running as root (containerized CI): permission bits cannot
                // make the dir unreadable; the assertion below would be
                // meaningless. Covered on every non-root target instead.
                let _ = fs::remove_dir_all(&input);
                return;
            }
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
            let result = book_paths(&input);
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(result, Err(SealError::Io { .. })));
            let _ = fs::remove_dir_all(&input);
        }
    }

    #[test]
    fn seal_is_deterministic_over_fixture_tree() {
        let input = unique_dir("det-input");
        write_fixture_tree(&input);

        let entries = paths_to_entries(&input, book_paths(&input).unwrap());
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

        // Exact-bytes leaf contract: the CAS object for a known book equals the
        // input file bytes, hash-addressed by the file's own BLAKE3.
        let book = input.join("train/10.txt");
        let book_bytes = fs::read(&book).unwrap();
        let digest = hex::encode_32(blake3::hash(&book_bytes).as_bytes());
        let cas_path = ws1
            .join(".attestrum")
            .join("cas")
            .join("blake3")
            .join(&digest[0..2])
            .join(&digest[2..4])
            .join(format!("{digest}.bin"));
        assert_eq!(
            fs::read(&cas_path).unwrap(),
            book_bytes,
            "CAS object must be the exact book file bytes"
        );

        // merkle.root sibling: 64 lowercase hex + newline, identical across runs.
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
