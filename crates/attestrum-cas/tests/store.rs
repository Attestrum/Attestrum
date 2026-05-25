//! Integration tests for `attestrum_cas::CasStore`. Uses
//! `CARGO_TARGET_TMPDIR` so test fixtures live under `target/tmp/`
//! and never leak into `/tmp`. Each test scopes its CAS root by name
//! so parallel test execution doesn't collide.

use std::fs;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use attestrum_cas::{stream_hash, CasStore};

/// Per-test counter; combined with the test name to scope each test's
/// CAS root under `CARGO_TARGET_TMPDIR`. Cleans the dir on entry so a
/// re-run starts fresh.
static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cas-e6-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

#[test]
fn new_creates_cas_and_tmp_subdirs() {
    let root = fresh_root("new_creates_subdirs");
    let _store = CasStore::new(&root).expect("create CasStore");
    assert!(root.join("cas").join("blake3").is_dir());
    assert!(root.join("tmp").is_dir());
}

#[test]
fn put_then_open_roundtrip() {
    let root = fresh_root("roundtrip");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"hello, attestrum-cas e6";
    let digest = *blake3::hash(contents).as_bytes();
    store.put(&digest, contents).expect("put");
    let mut file = store.open(&digest).expect("open");
    let mut read_back = Vec::new();
    file.read_to_end(&mut read_back).expect("read");
    assert_eq!(read_back, contents);
}

#[test]
fn put_is_idempotent_for_same_digest() {
    let root = fresh_root("idempotent");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"twice is once";
    let digest = *blake3::hash(contents).as_bytes();
    store.put(&digest, contents).expect("first put");
    store.put(&digest, contents).expect("second put no-op");
    assert!(store.exists(&digest).unwrap());
}

#[test]
fn sharding_lands_in_correct_two_level_dir() {
    let root = fresh_root("sharding");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"sharding probe";
    let digest = *blake3::hash(contents).as_bytes();
    let hex = attestrum_core::hex::encode_32(&digest);
    let expected = root
        .join("cas")
        .join("blake3")
        .join(&hex[..2])
        .join(&hex[2..4])
        .join(format!("{hex}.bin"));
    assert_eq!(store.path_for(&digest), expected);
    store.put(&digest, contents).expect("put");
    assert!(expected.exists());
}

struct ShardPair {
    a_bytes: Vec<u8>,
    a_digest: [u8; 32],
    b_bytes: Vec<u8>,
    b_digest: [u8; 32],
}

#[test]
fn multiple_digests_coexist_in_same_shard() {
    let root = fresh_root("same_shard");
    let store = CasStore::new(&root).expect("create CasStore");
    // Find two distinct inputs whose BLAKE3 digests share the first
    // two bytes (same <ab>/<cd>/ shard) so the test exercises a real
    // shared-leaf-dir scenario rather than two unrelated paths.
    let mut found: Option<ShardPair> = None;
    'outer: for i in 0u32..200_000 {
        let a_bytes = format!("alpha-{i}").into_bytes();
        let a_digest = *blake3::hash(&a_bytes).as_bytes();
        for j in (i + 1)..(i + 5_000) {
            let b_bytes = format!("beta-{j}").into_bytes();
            let b_digest = *blake3::hash(&b_bytes).as_bytes();
            if a_digest[0] == b_digest[0] && a_digest[1] == b_digest[1] && a_digest != b_digest {
                found = Some(ShardPair {
                    a_bytes: a_bytes.clone(),
                    a_digest,
                    b_bytes,
                    b_digest,
                });
                break 'outer;
            }
        }
    }
    let pair = found.expect(
        "expected to find two inputs whose BLAKE3 digests share the first two bytes within the search budget",
    );
    store.put(&pair.a_digest, &pair.a_bytes).expect("put a");
    store.put(&pair.b_digest, &pair.b_bytes).expect("put b");
    let pa = store.path_for(&pair.a_digest);
    let pb = store.path_for(&pair.b_digest);
    assert_eq!(pa.parent(), pb.parent());
    assert!(pa.exists() && pb.exists());
}

#[test]
fn exists_reflects_presence() {
    let root = fresh_root("exists");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"exists check";
    let digest = *blake3::hash(contents).as_bytes();
    assert!(!store.exists(&digest).unwrap());
    store.put(&digest, contents).expect("put");
    assert!(store.exists(&digest).unwrap());
}

#[test]
fn open_missing_digest_is_not_found() {
    let root = fresh_root("open_missing");
    let store = CasStore::new(&root).expect("create CasStore");
    let absent = [0xeeu8; 32];
    let err = store.open(&absent).expect_err("expected NotFound");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn concurrent_put_same_digest_races_safely() {
    let root = fresh_root("concurrent_same");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents: &'static [u8] = b"raced content for concurrent put test";
    let digest = *blake3::hash(contents).as_bytes();

    let threads: Vec<_> = (0..4)
        .map(|_| {
            let store = store.clone();
            thread::spawn(move || store.put(&digest, contents))
        })
        .collect();
    for t in threads {
        t.join().expect("thread join").expect("put");
    }

    assert!(store.exists(&digest).unwrap());
    let mut file = store.open(&digest).expect("open");
    let mut read_back = Vec::new();
    file.read_to_end(&mut read_back).expect("read");
    assert_eq!(read_back, contents);
}

#[test]
fn temp_dir_is_empty_after_successful_put() {
    let root = fresh_root("tmp_clean");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"temp-clean";
    let digest = *blake3::hash(contents).as_bytes();
    store.put(&digest, contents).expect("put");
    let temp_entries: Vec<_> = fs::read_dir(root.join("tmp"))
        .expect("read tmp")
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        temp_entries.is_empty(),
        "tmp dir should be empty after successful put, found {} entries",
        temp_entries.len()
    );
}

#[test]
fn stream_hash_then_put_roundtrip() {
    let root = fresh_root("stream_hash_put");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"stream + put end-to-end";
    let hash = stream_hash(&contents[..]).expect("stream_hash");
    store.put(&hash.blake3, contents).expect("put");
    let mut file = store.open(&hash.blake3).expect("open");
    let mut read_back = Vec::new();
    file.read_to_end(&mut read_back).expect("read");
    assert_eq!(read_back, contents);
}

#[cfg(unix)]
#[test]
#[cfg_attr(
    target_env = "musl",
    ignore = "Alpine CI runs as root with CAP_DAC_OVERRIDE which bypasses \
              directory write permissions, so chmod 0o555 is not enforced \
              against the test's UID. The general I/O-error-propagation \
              invariant is covered by missing_tmp_dir_propagates_io_error \
              (below), which works regardless of UID because root cannot \
              open a file in a directory that does not exist."
)]
fn read_only_parent_propagates_io_error() {
    use std::os::unix::fs::PermissionsExt;
    let root = fresh_root("readonly_parent");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"readonly test";
    let digest = *blake3::hash(contents).as_bytes();

    // Compute the shard dir, create it, then chmod it to read+exec
    // only so file creation inside it fails.
    let shard_dir = store
        .path_for(&digest)
        .parent()
        .expect("path_for parent")
        .to_path_buf();
    fs::create_dir_all(&shard_dir).expect("mkdir shard");
    let original = fs::metadata(&shard_dir).expect("stat shard").permissions();
    fs::set_permissions(&shard_dir, fs::Permissions::from_mode(0o555)).expect("chmod shard ro");

    // The temp write happens in `tmp/` (still writable), but the
    // rename target is inside the read-only shard dir — fails on
    // Linux/macOS with EACCES / EPERM.
    let result = store.put(&digest, contents);

    // Restore perms before asserting so the test runner can clean up.
    fs::set_permissions(&shard_dir, original).expect("chmod shard restore");

    let err = result.expect_err("expected put to fail with read-only shard dir");
    assert!(
        matches!(err.kind(), ErrorKind::PermissionDenied),
        "expected PermissionDenied, got {:?} — {err}",
        err.kind()
    );
}

#[test]
fn missing_tmp_dir_propagates_io_error() {
    // Verifies that filesystem errors during `CasStore::put` propagate as
    // io::Error rather than being swallowed. Exercises the temp-file-create
    // path inside put (OpenOptions::create_new in the `tmp/` subdir).
    //
    // Uses a missing parent directory rather than chmod-based permissions
    // so the assertion holds regardless of UID — root cannot open a file
    // in a directory that does not exist (kernel structural error, not
    // a DAC check that CAP_DAC_OVERRIDE could bypass). This is the
    // cross-target equivalent of `read_only_parent_propagates_io_error`,
    // which is musl-ignored because Alpine CI runs as root.
    let root = fresh_root("missing_tmp");
    let store = CasStore::new(&root).expect("create CasStore");
    let contents = b"missing tmp dir test";
    let digest = *blake3::hash(contents).as_bytes();

    // Remove the `tmp/` subdir that CasStore::new just created. The next
    // `put` call tries to OpenOptions::create_new("tmp/.attestrum-tmp.<...>")
    // and gets ENOENT.
    fs::remove_dir_all(root.join("tmp")).expect("remove tmp/");

    let err = store
        .put(&digest, contents)
        .expect_err("expected put to fail when tmp/ is absent");
    assert!(
        matches!(err.kind(), ErrorKind::NotFound),
        "expected NotFound (tmp/ removed), got {:?} — {err}",
        err.kind()
    );
}

/// Sprint 4 E3.6: regression test for the temp-filename codepath.
///
/// Two separate `CasStore` instances, each given the same 100 inputs,
/// must produce identical sets of final hash-addressed filenames, and
/// must leave `tmp/` empty after all puts complete. The temp filename
/// itself is process-scoped (PID + counter) so the literal names will
/// differ across runs — but the FINAL filenames are content-addressed
/// and must be byte-identical because BLAKE3 is deterministic.
///
/// This protects against future regressions that re-introduce
/// non-determinism into the CAS write path (e.g., a temp filename
/// component leaking into the final path, or a temp file left behind
/// after a successful rename).
#[test]
fn cross_instance_determinism_and_clean_temp_dir() {
    fn write_corpus(label: &str) -> (PathBuf, Vec<[u8; 32]>) {
        let root = fresh_root(label);
        let store = CasStore::new(&root).expect("create CasStore");
        let mut digests: Vec<[u8; 32]> = Vec::with_capacity(100);
        for i in 0..100u32 {
            let contents = format!("attestrum-cas-e3.6-determinism-doc-{i:03}").into_bytes();
            let digest = *blake3::hash(&contents).as_bytes();
            store
                .put(&digest, &contents)
                .unwrap_or_else(|e| panic!("put #{i} failed: {e}"));
            digests.push(digest);
        }
        (root, digests)
    }

    let (root_a, digests_a) = write_corpus("determinism_a");
    let (root_b, digests_b) = write_corpus("determinism_b");

    // The digests must match exactly — BLAKE3 is deterministic and the
    // inputs are identical. If this fails, the input-construction path
    // itself drifted; the CAS isn't the cause.
    assert_eq!(
        digests_a, digests_b,
        "input-derived BLAKE3 digests diverged across runs — input construction is non-deterministic, \
         not the CAS"
    );

    // Walk each store's blake3 tree and collect every leaf filename
    // relative to the `cas/blake3/` root, then sort. Both sets must be
    // exactly equal — the final paths are content-addressed and the
    // inputs match, so any divergence means the CAS write path is
    // leaking non-determinism into the on-disk layout.
    fn collect_blake3_leaves(root: &std::path::Path) -> Vec<PathBuf> {
        let blake3_root = root.join("cas").join("blake3");
        let mut out = Vec::new();
        for entry in walkdir(&blake3_root) {
            if entry.is_file() {
                out.push(
                    entry
                        .strip_prefix(&blake3_root)
                        .expect("entry under blake3 root")
                        .to_path_buf(),
                );
            }
        }
        out.sort();
        out
    }

    let leaves_a = collect_blake3_leaves(&root_a);
    let leaves_b = collect_blake3_leaves(&root_b);
    assert_eq!(leaves_a.len(), 100, "expected 100 leaves under cas/blake3/");
    assert_eq!(
        leaves_a, leaves_b,
        "CAS leaf set diverged across two stores given the same inputs — final filenames are content-addressed \
         and must be byte-identical"
    );

    // Both tmp/ dirs must be empty — no `.attestrum-tmp.*` leftovers from
    // successful puts. (A leaked temp would indicate the rename failed
    // silently OR temp-cleanup got skipped on a path that should clean.)
    for (label, root) in [("a", &root_a), ("b", &root_b)] {
        let tmp = root.join("tmp");
        let leftovers: Vec<_> = fs::read_dir(&tmp)
            .expect("read tmp dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert!(
            leftovers.is_empty(),
            "tmp/ in store {label} not clean after 100 puts; leftovers: {leftovers:?}"
        );
    }
}

/// Local mini-walker so the test doesn't depend on `walkdir` (kept
/// dep-free per CLAUDE.md §8 — only pre-approved deps). Returns every
/// file/dir under `root` recursively.
fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.push(path);
        }
    }
    out
}
