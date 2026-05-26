//! Cross-target byte-determinism gate for `attestrum-fingerprint`.
//!
//! Lands at Sprint 5 S5-D1 E5. Asserts that fingerprinting a fixed set
//! of inputs (committed PNG fixtures + a literal UTF-8 text snippet)
//! with a fixed `source_date_epoch` produces byte-identical canonical
//! JSON across runs. The existing `.github/workflows/determinism.yml`
//! 4-target matrix (`cargo test --workspace` on Linux glibc x86_64,
//! Linux glibc aarch64, macOS darwin aarch64, Alpine musl x86_64)
//! catches inter-target drift on the same golden — if any target
//! produces different JSON than the committed file under
//! `tests/golden/`, the test fails on that target and the matrix
//! surfaces the drift in CI.
//!
//! Why goldens rather than only `assert_eq!(a, b)` within a single
//! run: within-run equality is necessary but not sufficient. The
//! load-bearing property is cross-target byte-identity; a committed
//! golden is the only way to surface jitter that's deterministic
//! within one target but differs across targets (e.g., a float-DCT
//! that happens to produce stable output on every macOS run but
//! differs on Linux musl).
//!
//! PNG fixtures under `tests/fixtures/` are committed binary files
//! (each ~few hundred bytes). The `cargo-deny` `sources` check + the
//! pre-commit secret-scanner are both fine with binary blobs in
//! tests/fixtures (the existing `tests/golden/article53/` precedent
//! sets the pattern).
//!
//! Regen golden JSON files via
//! `ATTESTRUM_REGEN_FINGERPRINT_GOLDEN=1 cargo test -p attestrum-fingerprint
//! --test determinism`. Regenerate the PNG fixtures via
//! `ATTESTRUM_REGEN_FINGERPRINT_FIXTURES=1` (separate env var so a
//! benign golden-regen doesn't silently rewrite the fixtures).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use attestrum_fingerprint::{fingerprint_image, fingerprint_text, FingerprintOpts};

/// Consistent epoch across all goldens — RFC 3339 form `2025-05-24T18:00:00Z`.
/// Matches the test-module convention in `src/lib.rs` (lib.rs:586).
const FIXED_EPOCH: i64 = 1_748_109_600;

/// Literal text snippet used for the text determinism golden. Chosen to
/// exercise all three PROTECTED text-pipeline steps (NFC normalization,
/// case folding, whitespace collapse) plus produce a MinHash with
/// distinct values for several permutations.
const TEXT_FIXTURE: &[u8] =
    b"The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.";

fn opts() -> FingerprintOpts {
    FingerprintOpts {
        source_date_epoch: FIXED_EPOCH,
    }
}

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> PathBuf {
    crate_dir().join("tests/fixtures")
}

fn goldens_dir() -> PathBuf {
    crate_dir().join("tests/golden")
}

/// Recursively sort the keys of every JSON object so the pretty-printed
/// output is byte-stable across runs. Mirrors the helper in
/// `tests/schema_derive.rs`.
fn sort_keys(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, Value> =
                std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, sort_keys(v));
            }
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Serialize an `impl Serialize` to canonical pretty JSON with a trailing
/// newline — the exact byte form that gets diffed against the golden.
fn canonical_pretty<T: serde::Serialize>(value: &T) -> String {
    let v = serde_json::to_value(value).expect("serialize to Value");
    let sorted = sort_keys(v);
    let mut text = serde_json::to_string_pretty(&sorted).expect("Value -> pretty string");
    text.push('\n');
    text
}

fn check_or_regen_golden(file_name: &str, derived: &str) {
    let path = goldens_dir().join(file_name);
    if env::var("ATTESTRUM_REGEN_FINGERPRINT_GOLDEN").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
        }
        fs::write(&path, derived).unwrap_or_else(|e| panic!("regen write {}: {e}", path.display()));
        eprintln!("regenerated {}", path.display());
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read golden {}: {e}\nHint: ATTESTRUM_REGEN_FINGERPRINT_GOLDEN=1 cargo test -p attestrum-fingerprint --test determinism",
            path.display()
        )
    });

    if derived != expected {
        let diff = format!(
            "Fingerprint bundle JSON differs from committed golden {}.\n\n  This is the load-bearing cross-target determinism gate at S5-D1 E5.\n  Drift here means a fingerprint produced on one CI target byte-differs from\n  another target's fingerprint of the same input. Inclusion proofs are\n  invalidated by any such drift.\n\n  Derived (first 400 chars):\n{}\n\n  Committed (first 400 chars):\n{}\n\nIf this drift is intentional (e.g., a deliberate PROTECTED-system-change\nwith a v0.2 URI bump): regen via\n  ATTESTRUM_REGEN_FINGERPRINT_GOLDEN=1 cargo test -p attestrum-fingerprint --test determinism\nand include the byte delta in the commit body with the\n`Protected-system-change: approved-by=...` footer per CLAUDE.md §4.\n",
            path.display(),
            derived.chars().take(400).collect::<String>(),
            expected.chars().take(400).collect::<String>(),
        );
        panic!("{diff}");
    }
}

#[test]
fn text_fingerprint_bundle_matches_committed_golden() {
    let bundle = fingerprint_text(TEXT_FIXTURE, &opts())
        .expect("fingerprint_text on literal UTF-8 fixture should succeed");
    let derived = canonical_pretty(&bundle);
    check_or_regen_golden("text-fixture.bundle.json", &derived);
}

#[test]
fn checkerboard_image_fingerprint_bundle_matches_committed_golden() {
    let png = fs::read(fixtures_dir().join("checkerboard.png"))
        .expect("read tests/fixtures/checkerboard.png");
    let bundle = fingerprint_image(&png, &opts())
        .expect("fingerprint_image on committed PNG fixture should succeed");
    let derived = canonical_pretty(&bundle);
    check_or_regen_golden("checkerboard-fixture.bundle.json", &derived);
}

#[test]
fn gradient_image_fingerprint_bundle_matches_committed_golden() {
    let png =
        fs::read(fixtures_dir().join("gradient.png")).expect("read tests/fixtures/gradient.png");
    let bundle = fingerprint_image(&png, &opts())
        .expect("fingerprint_image on committed PNG fixture should succeed");
    let derived = canonical_pretty(&bundle);
    check_or_regen_golden("gradient-fixture.bundle.json", &derived);
}

#[test]
fn intra_run_determinism_for_text_fingerprint() {
    // Within-run determinism — necessary precondition for cross-target
    // byte-identity. If this fails, the issue is a non-determinism
    // source inside the crate (e.g., a HashMap iteration order leaking
    // through), not a cross-target jitter.
    let a = fingerprint_text(TEXT_FIXTURE, &opts()).unwrap();
    let b = fingerprint_text(TEXT_FIXTURE, &opts()).unwrap();
    assert_eq!(a, b);
    let a_json = canonical_pretty(&a);
    let b_json = canonical_pretty(&b);
    assert_eq!(a_json, b_json);
}

#[test]
fn intra_run_determinism_for_image_fingerprint() {
    let png = fs::read(fixtures_dir().join("checkerboard.png"))
        .expect("read tests/fixtures/checkerboard.png");
    let a = fingerprint_image(&png, &opts()).unwrap();
    let b = fingerprint_image(&png, &opts()).unwrap();
    assert_eq!(a, b);
    let a_json = canonical_pretty(&a);
    let b_json = canonical_pretty(&b);
    assert_eq!(a_json, b_json);
}

// ============================================================================
// PNG fixture regen — separate env var
// ============================================================================
//
// The PNG fixtures under `tests/fixtures/` are generated once via the
// existing in-test `image::ImageBuffer::from_fn` patterns inlined in
// `src/lib.rs`. They're committed to the repo as binary so the inputs
// are byte-stable inputs to the determinism tests above (a regenerated
// PNG from a slightly different image-crate version would shift byte-by-
// byte and invalidate the JSON goldens). Regen via
// `ATTESTRUM_REGEN_FINGERPRINT_FIXTURES=1 cargo test -p attestrum-fingerprint
// --test determinism`. Separate env var prevents accidental fixture
// rewrites during routine golden regens.

#[test]
fn regen_png_fixtures_when_env_var_is_set() {
    if env::var("ATTESTRUM_REGEN_FINGERPRINT_FIXTURES").is_err() {
        return;
    }
    use image::{DynamicImage, ImageBuffer, ImageFormat, Luma};
    use std::io::Cursor;

    let dim = 32u32;

    // checkerboard.png: 4-tile-size pattern, 8×8 cells.
    let buf: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(dim, dim, |x, y| {
        let cell_x = x / 4;
        let cell_y = y / 4;
        if (cell_x + cell_y) % 2 == 0 {
            Luma([255u8])
        } else {
            Luma([0u8])
        }
    });
    let img = DynamicImage::ImageLuma8(buf);
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("PNG encode checkerboard fixture");
    let dir = fixtures_dir();
    fs::create_dir_all(&dir).expect("create fixtures dir");
    fs::write(dir.join("checkerboard.png"), &out).expect("write checkerboard.png");
    eprintln!("regenerated {}", dir.join("checkerboard.png").display());

    // gradient.png: horizontal left-to-right gradient.
    let buf: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(dim, dim, |x, _y| {
        let v = (x * 255 / (dim - 1)).min(255) as u8;
        Luma([v])
    });
    let img = DynamicImage::ImageLuma8(buf);
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("PNG encode gradient fixture");
    fs::write(dir.join("gradient.png"), &out).expect("write gradient.png");
    eprintln!("regenerated {}", dir.join("gradient.png").display());
}
