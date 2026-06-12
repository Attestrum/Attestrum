//! `attestrum merge --inputs <files...> --out <merged.parquet>` — merges
//! N already-sealed shard manifests into one in a single streaming pass, with
//! memory bounded independent of the total row count.
//!
//! Each shard manifest is read in batches ([`ManifestBatchReader`]) and the
//! shards are k-way merged by `(document_id, shard_index)` into the canonical
//! `(document_id, occurrence_index)` on-disk order. As each row is emitted the
//! merge stamps `input_ordinal` = `shard_offset + within-shard position` (the
//! concat-order position the in-memory build assigned) and `occurrence_index`
//! via an O(1) running per-digest counter (equal digests are contiguous in the
//! merged stream), appends the row to a streaming Parquet writer
//! ([`ManifestWriter`]), and collects its `document_id` leaf for the root.
//!
//! This is BYTE-IDENTICAL to the previous load-everything / concat /
//! re-`assign_*` / `sort_entries` / `write_manifest` implementation for any set
//! of sorted shard manifests — every Attestrum-produced manifest is written in
//! `(document_id, occurrence_index)` order, so the merged stream emits rows in
//! exactly the order the old global sort produced, with the same
//! `input_ordinal` / `occurrence_index` values. The only buffers that grow with
//! the corpus are the leaf-digest vector (32 B/row) and one row group inside the
//! writer; the merge no longer holds every row in memory, so it scales to the
//! ~100M-row rungs where the old O(rows) merge exhausted the runner.
//!
//! **Precondition**: each shard manifest is internally sorted so equal
//! `document_id`s are contiguous (guaranteed by `sort_entries`, which every seal
//! path runs before writing). A shard whose `document_id`s are not
//! non-decreasing is rejected with [`MergeError::UnsortedShard`] rather than
//! silently producing a different result than the old merge.
//!
//! The merged Merkle root (RFC 6962 over BLAKE3 via
//! `attestrum_merkle::merkle_root` over the canonically sorted `document_id`
//! leaves) is printed as a `merkle_root:` line and written to a `merkle.root`
//! file beside `--out` (64 lowercase hex chars + newline, the `attestrum build`
//! sibling-file format). See `docs/diagrams/sprint-3/sharding.md` for the
//! determinism contract: the merged Merkle root ALWAYS equals the root of an
//! unsharded build of the same logical corpus (multiset Merkle is invariant
//! under within-group permutation). The merged `manifest.parquet` BYTES
//! additionally equal the unsharded variant only when `input_ordinal` happens
//! to align — see `sharding.md`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs;
use std::path::PathBuf;

use attestrum_manifest::{manifest_row_count, ManifestBatchReader, ManifestEntry, ManifestWriter};
use thiserror::Error;

/// How many merged entries to stage before handing a slice to the streaming
/// writer. Bounds the merge-side staging buffer; the writer does its own
/// row-group-aligned buffering internally, so this only batches the
/// `entries`-to-Arrow conversion call frequency.
const WRITE_CHUNK: usize = 8192;

/// Subcommand arguments.
#[derive(Debug)]
pub struct Args {
    pub inputs: Vec<PathBuf>,
    pub out: PathBuf,
}

/// Errors `merge::run` can surface. All map to exit code 1.
#[derive(Debug, Error)]
pub enum MergeError {
    #[error("--inputs must be non-empty; pass at least one shard manifest")]
    NoInputs,

    #[error("input manifest read failed at {path}: {source}")]
    InputRead {
        path: PathBuf,
        #[source]
        source: attestrum_core::AttestrumError,
    },

    #[error(
        "shard manifest {path} is not sorted by document_id (equal digests must \
         be contiguous); attestrum merge requires Attestrum-sealed manifests"
    )]
    UnsortedShard { path: PathBuf },

    #[error("output dir prepare failed at {path}: {source}")]
    OutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("merged manifest write failed: {0}")]
    Write(#[from] attestrum_core::AttestrumError),

    #[error("merkle.root write failed at {path}: {source}")]
    RootFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// `attestrum merge` entry point. Returns 0 on success, 1 on any error.
/// All errors are printed to stderr inside this function.
pub fn run(args: Args) -> u8 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("attestrum merge: {err}");
            let mut source = std::error::Error::source(&err);
            while let Some(s) = source {
                eprintln!("  caused by: {s}");
                source = std::error::Error::source(s);
            }
            1
        }
    }
}

/// One shard's streaming read cursor. The current batch is stored reversed so
/// `rev_batch.pop()` yields rows in on-disk order in O(1) without cloning.
struct ShardCursor {
    reader: ManifestBatchReader,
    /// Current batch, stored reversed: `pop()` returns the next row in order.
    rev_batch: Vec<ManifestEntry>,
    /// 0-based ordinal of the next row within this shard (across batches).
    j: u64,
    /// `input_ordinal` base for this shard = sum of prior shards' row counts.
    offset: u64,
    /// document_id of the last row popped — enforces within-shard sort order.
    prev_doc: Option<[u8; 32]>,
    /// Source path, for error context.
    path: PathBuf,
}

impl ShardCursor {
    /// Ensure `rev_batch` is non-empty unless the reader is exhausted. Skips
    /// any empty batches the reader may yield.
    fn fill(&mut self) -> Result<(), MergeError> {
        while self.rev_batch.is_empty() {
            match self.reader.next() {
                None => break,
                Some(Ok(mut batch)) => {
                    batch.reverse();
                    self.rev_batch = batch;
                }
                Some(Err(source)) => {
                    return Err(MergeError::InputRead {
                        path: self.path.clone(),
                        source,
                    });
                }
            }
        }
        Ok(())
    }

    /// document_id of the head row (next to emit), or None if exhausted.
    fn peek_doc(&self) -> Option<[u8; 32]> {
        self.rev_batch.last().map(|e| e.document_id)
    }
}

fn run_inner(args: Args) -> Result<(), MergeError> {
    if args.inputs.is_empty() {
        return Err(MergeError::NoInputs);
    }

    // Lex-sort input paths. The concat order this defines is what assigns
    // input_ordinal (and, via contiguity, occurrence_index), matching the
    // previous implementation's `sorted_inputs` walk.
    let mut sorted_inputs = args.inputs.clone();
    sorted_inputs.sort();

    // Open one cursor per shard; compute each shard's input_ordinal offset from
    // the footer row count (no row-group decode).
    let mut cursors: Vec<ShardCursor> = Vec::with_capacity(sorted_inputs.len());
    let mut offset: u64 = 0;
    for path in &sorted_inputs {
        let count = manifest_row_count(path).map_err(|source| MergeError::InputRead {
            path: path.clone(),
            source,
        })?;
        let reader = ManifestBatchReader::open(path).map_err(|source| MergeError::InputRead {
            path: path.clone(),
            source,
        })?;
        let mut cursor = ShardCursor {
            reader,
            rev_batch: Vec::new(),
            j: 0,
            offset,
            prev_doc: None,
            path: path.clone(),
        };
        cursor.fill()?;
        cursors.push(cursor);
        offset += count;
    }
    let total_rows = offset;

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| MergeError::OutputDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let mut writer = ManifestWriter::create(&args.out)?;

    // Min-heap of (document_id, shard_index): popping the global minimum emits
    // all rows sharing a document_id contiguously (lower shard_index first =
    // concat order), which is exactly the canonical (document_id,
    // occurrence_index) on-disk order.
    let mut heap: BinaryHeap<Reverse<([u8; 32], usize)>> = BinaryHeap::new();
    for (idx, cursor) in cursors.iter().enumerate() {
        if let Some(doc) = cursor.peek_doc() {
            heap.push(Reverse((doc, idx)));
        }
    }

    let mut leaves: Vec<[u8; 32]> =
        Vec::with_capacity(usize::try_from(total_rows).unwrap_or(usize::MAX));
    let mut out_buf: Vec<ManifestEntry> = Vec::with_capacity(WRITE_CHUNK);
    let mut cur_doc: Option<[u8; 32]> = None;
    let mut occ: u32 = 0;
    let mut rows: u64 = 0;

    while let Some(Reverse((doc, idx))) = heap.pop() {
        let cursor = &mut cursors[idx];

        // Within-shard order guard: document_id must be non-decreasing, or the
        // contiguity the occurrence_index counter relies on is violated.
        if let Some(prev) = cursor.prev_doc {
            if doc < prev {
                return Err(MergeError::UnsortedShard {
                    path: cursor.path.clone(),
                });
            }
        }

        let mut entry = cursor
            .rev_batch
            .pop()
            .expect("heap invariant: popped shard has a head row");
        cursor.prev_doc = Some(doc);

        // input_ordinal = concat position (shard offset + within-shard ordinal).
        entry.input_ordinal = cursor.offset + cursor.j;
        cursor.j += 1;

        // occurrence_index = running per-digest counter over the contiguous
        // (document_id-sorted) merged stream.
        if cur_doc == Some(entry.document_id) {
            occ += 1;
        } else {
            cur_doc = Some(entry.document_id);
            occ = 0;
        }
        entry.occurrence_index = occ;

        leaves.push(entry.document_id);
        out_buf.push(entry);
        rows += 1;
        if out_buf.len() >= WRITE_CHUNK {
            writer.write_entries(&out_buf)?;
            out_buf.clear();
        }

        // Advance this shard and re-push its new head.
        cursor.fill()?;
        if let Some(next_doc) = cursor.peek_doc() {
            heap.push(Reverse((next_doc, idx)));
        }
    }
    if !out_buf.is_empty() {
        writer.write_entries(&out_buf)?;
    }
    writer.close()?;

    // The emission order IS the canonical (document_id, occurrence_index) leaf
    // order, so this root equals the unsharded build's by multiset invariance.
    let root = attestrum_merkle::merkle_root(&leaves);
    let root_hex = hex_64(&root);

    // Sibling artifact beside the merged manifest, same format as
    // `attestrum build`'s `merkle.root`: 64 lowercase hex chars + newline.
    let root_path = args.out.with_file_name("merkle.root");
    fs::write(&root_path, format!("{root_hex}\n")).map_err(|source| MergeError::RootFile {
        path: root_path.clone(),
        source,
    })?;

    tracing::info!(
        inputs = sorted_inputs.len(),
        out = %args.out.display(),
        rows,
        merkle_root = %root_hex,
        "merge complete"
    );
    println!("attestrum merge: ok");
    println!("  inputs:       {}", sorted_inputs.len());
    println!("  rows:         {rows}");
    println!("  merkle_root:  {root_hex}");
    println!("  merkle_file:  {}", root_path.display());
    println!("  out:          {}", args.out.display());
    Ok(())
}

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
