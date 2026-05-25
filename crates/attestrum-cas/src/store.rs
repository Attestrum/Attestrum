//! Content-addressed store rooted at a chosen directory (typically
//! `<workspace>/.attestrum/`).
//!
//! On-disk layout — PROTECTED per CLAUDE.md §4 (matches
//! `PATH-A-BRIEF.md` §1.9 and `docs/diagrams/overview/cas-layout.md`):
//!
//! ```text
//! <root>/
//!   cas/blake3/<ab>/<cd>/<hex-digest>.bin    # canonical content path
//!   tmp/.attestrum-tmp.<pid>-<counter>              # atomic-rename staging
//! ```
//!
//! The two-level hex sharding (`<ab>/<cd>/`) mirrors git's object DB
//! and caps any single leaf directory at ~65 k entries, well within
//! ext4 / APFS dirent-scan thresholds. `<hex-digest>` is the full
//! 64-character lowercase BLAKE3 hex (sharding bytes are repeated in
//! the final filename so the path is self-describing).
//!
//! `put` writes into `tmp/` with a unique per-call filename, fsyncs
//! the file, `rename(2)`s atomically to the final path (POSIX-atomic
//! on the same filesystem), then fsyncs the final-path's parent
//! directory so the rename itself survives a power loss after return.
//! If a concurrent writer landed the same digest first (race lost at
//! rename, or final path observed up-front), the call short-circuits
//! idempotently. BLAKE3's collision resistance lets us trust that
//! same digest implies same content.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::hex;

/// The CAS-internal subdirectory holding BLAKE3-addressed content.
/// Relative to the store's root.
const CAS_SUBDIR: &str = "cas";
/// The hash-algorithm subdirectory under `cas/`.
const BLAKE3_SUBDIR: &str = "blake3";
/// The atomic-rename staging directory at the store root. Must be on
/// the same filesystem as `cas/blake3/` for `rename(2)` to be atomic.
const TMP_SUBDIR: &str = "tmp";
/// File extension appended to every CAS object (matches
/// `PATH-A-BRIEF.md` §1.9: `cas/blake3/aa/bb/<full-hash>.bin`).
const OBJECT_EXT: &str = "bin";
/// Prefix for temp filenames so concurrent writers and stale temps
/// from prior crashed processes are easy to identify and clean up.
const TEMP_PREFIX: &str = ".attestrum-tmp.";

/// Monotonic per-process counter, combined with PID to build
/// collision-free temp filenames across threads and processes. The PID
/// disambiguates concurrent processes; the counter disambiguates calls
/// within a process. Sprint 4 E3.6 dropped the `SystemTime::now()`
/// nanosecond component that previously also appeared in the name —
/// it added no real uniqueness (PID + monotonic AtomicU64 is already
/// collision-free) and violated the CLAUDE.md §7 rule that all
/// determinism-relevant timestamps come from `--source-date-epoch`.
/// Temp filenames do not enter output bytes (the file is atomically
/// renamed to its content-addressed final name before close), so this
/// was not an active determinism bug — but it was one copy-paste away
/// from one. See determinism audit 2026-05-24.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Content-addressed store. Cheap to clone (just clones a `PathBuf`)
/// — the file-system layout itself carries all state.
#[derive(Clone, Debug)]
pub struct CasStore {
    root: PathBuf,
}

impl CasStore {
    /// Open (or create) a CAS rooted at `root`. The `cas/blake3/` and
    /// `tmp/` subdirectories are created if missing.
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join(CAS_SUBDIR).join(BLAKE3_SUBDIR))?;
        fs::create_dir_all(root.join(TMP_SUBDIR))?;
        Ok(Self { root })
    }

    /// Root directory of the store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Canonical on-disk path for a given BLAKE3 digest.
    pub fn path_for(&self, blake3_digest: &[u8; 32]) -> PathBuf {
        let hex_digest = hex::encode_32(blake3_digest);
        let mut p = self.root.join(CAS_SUBDIR).join(BLAKE3_SUBDIR);
        p.push(&hex_digest[..2]);
        p.push(&hex_digest[2..4]);
        p.push(format!("{hex_digest}.{OBJECT_EXT}"));
        p
    }

    /// Returns `true` when an object with `blake3_digest` is present
    /// in the store.
    pub fn exists(&self, blake3_digest: &[u8; 32]) -> io::Result<bool> {
        Ok(self.path_for(blake3_digest).exists())
    }

    /// Open an existing object for reading. Returns
    /// `io::ErrorKind::NotFound` if the digest is not in the store.
    pub fn open(&self, blake3_digest: &[u8; 32]) -> io::Result<File> {
        File::open(self.path_for(blake3_digest))
    }

    /// Atomically write `contents` under the digest's content-addressed
    /// path. The caller is responsible for computing `blake3_digest`
    /// from `contents` (typically via [`crate::stream_hash`]); `put`
    /// does NOT re-hash to verify.
    ///
    /// Idempotent: if the final path already exists (either because we
    /// already wrote it, or because a concurrent writer landed the same
    /// digest first), the call succeeds without rewriting. BLAKE3's
    /// collision resistance makes "same digest ⇒ same content" a safe
    /// assumption.
    ///
    /// Atomicity contract: a concurrent reader observing the final
    /// path sees either no file or the fully-written file — never a
    /// partially-written one. The temp file is briefly observable in
    /// the `tmp/` subdirectory during the write window but is never
    /// observable at the final path. This satisfies the resumability
    /// property `attestrum build` needs after a crash.
    pub fn put(&self, blake3_digest: &[u8; 32], contents: &[u8]) -> io::Result<()> {
        let final_path = self.path_for(blake3_digest);
        let final_parent = final_path
            .parent()
            .expect("path_for always yields a final path with a parent");

        // Fast path: already present (own write, concurrent writer, or
        // a prior crashed put that completed the rename). Skip work.
        if final_path.exists() {
            return Ok(());
        }

        // Ensure the leaf shard directory exists. `cas/blake3/` itself
        // was created by `new`; `<ab>/<cd>/` may not be.
        fs::create_dir_all(final_parent)?;

        let temp_path = self.root.join(TMP_SUBDIR).join(unique_temp_name());

        // Write + fsync the contents into the temp file. `create_new`
        // ensures we never clobber a temp from a previous crashed
        // process — the (pid, counter) combo makes a real collision
        // astronomically unlikely (the OS rolls PIDs, but the counter
        // resets per process so a rolled PID hitting the exact same
        // counter value on the exact same temp file requires a crash
        // mid-put + dirty temp file + identical-PID future process +
        // identical call-order to that PID), and the create_new flag
        // makes the failure mode loud rather than silent if it ever
        // does happen.
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)?;
            file.write_all(contents)?;
            file.sync_all()?;
        }

        match fs::rename(&temp_path, &final_path) {
            Ok(()) => {
                // Fsync the final path's parent directory so the rename
                // itself is durable across a power loss. Best-effort —
                // some filesystems (notably some on macOS) reject
                // fsync on directories; in those cases we accept the
                // weaker guarantee rather than fail the put.
                if let Ok(dir) = File::open(final_parent) {
                    let _ = dir.sync_all();
                }
                Ok(())
            }
            Err(rename_err) => {
                // Best-effort temp cleanup so a failed rename doesn't
                // leak stale temps. Failing to clean up is non-fatal.
                let _ = fs::remove_file(&temp_path);
                // If the rename failed because a concurrent writer
                // landed the same digest first, that's still success:
                // the final path is present and BLAKE3 collision
                // resistance means the content matches.
                if final_path.exists() {
                    Ok(())
                } else {
                    Err(rename_err)
                }
            }
        }
    }
}

fn unique_temp_name() -> String {
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{TEMP_PREFIX}{pid}-{counter}")
}
