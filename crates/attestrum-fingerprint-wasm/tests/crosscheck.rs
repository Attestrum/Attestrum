//! Native cross-check: the `extern "C"` export, the kernel it wraps, and
//! `attestrum-fingerprint`'s PUBLIC `fingerprint_text` path must all agree with
//! the committed golden (`tests/golden/minhash-vectors.txt`).
//!
//! This is the native half of the byte-identity chain. The wasm half — proving
//! the actual compiled `.wasm` also matches the golden — is the
//! `wasm-crosscheck` CI gate (`tools/wasm-crosscheck/run.mjs`). Together they
//! prove the browser's near-match answer is byte-identical to the CLI's, with
//! no implementation that could drift.
//!
//! Regenerate the golden after an intentional passage change:
//! `cargo run -p attestrum-fingerprint-wasm --example gen_golden > <golden>`.

use attestrum_fingerprint::{fingerprint_text, FingerprintOpts};
use attestrum_fingerprint_wasm::{attestrum_minhash, MINHASH_PERMS};
use attestrum_text_minhash::{minhash, normalize_text};

/// (label, input, expected 128 `u64`) parsed from the golden file.
fn golden_vectors() -> Vec<(String, String, Vec<u64>)> {
    let raw = include_str!("golden/minhash-vectors.txt");
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.split('\t');
        let label = cols.next().expect("label column");
        let input = cols.next().expect("input column");
        let hexes = cols.next().expect("hex column");
        assert!(cols.next().is_none(), "exactly 3 tab-separated columns");

        let sig: Vec<u64> = hexes
            .split(',')
            .map(|h| u64::from_str_radix(h, 16).expect("16-char hex u64"))
            .collect();
        assert_eq!(
            sig.len(),
            MINHASH_PERMS,
            "golden line {label} must carry {MINHASH_PERMS} permutations"
        );
        out.push((label.to_string(), input.to_string(), sig));
    }
    out
}

/// Drive the raw `extern "C"` export the way the browser does: input bytes in,
/// 128 little-endian `u64` out.
fn minhash_via_extern(input: &str) -> Vec<u64> {
    let bytes = input.as_bytes();
    let mut out = vec![0u8; MINHASH_PERMS * 8];
    // SAFETY: `bytes` is a valid readable range; `out` is writable for
    // MINHASH_PERMS * 8 bytes. Mirrors the alloc/call/read pattern the JS glue
    // and the Node cross-check loader use.
    let written = unsafe { attestrum_minhash(bytes.as_ptr(), bytes.len(), out.as_mut_ptr()) };
    assert_eq!(written, MINHASH_PERMS * 8, "must write exactly 1024 bytes");
    out.chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[test]
fn golden_file_is_well_formed() {
    let vectors = golden_vectors();
    assert!(
        vectors.len() >= 5,
        "expected the curated passage set, got {}",
        vectors.len()
    );
    // The empty passage anchors the all-MAX path.
    let empty = vectors
        .iter()
        .find(|(l, ..)| l == "empty")
        .expect("empty passage present");
    assert_eq!(empty.2, vec![u64::MAX; MINHASH_PERMS]);
}

#[test]
fn kernel_matches_golden() {
    for (label, input, expected) in golden_vectors() {
        let got = minhash::compute(&normalize_text(&input));
        assert_eq!(got, expected, "kernel drift on passage `{label}`");
    }
}

#[test]
fn extern_export_matches_golden() {
    for (label, input, expected) in golden_vectors() {
        let got = minhash_via_extern(&input);
        assert_eq!(got, expected, "extern export drift on passage `{label}`");
    }
}

#[test]
fn fingerprint_public_api_matches_golden() {
    let opts = FingerprintOpts {
        source_date_epoch: 0,
    };
    for (label, input, expected) in golden_vectors() {
        let bundle = fingerprint_text(input.as_bytes(), &opts)
            .unwrap_or_else(|e| panic!("fingerprint_text failed on `{label}`: {e}"));
        let got = bundle
            .text
            .unwrap_or_else(|| panic!("no text fingerprint for `{label}`"))
            .minhash;
        assert_eq!(
            got, expected,
            "public fingerprint_text drift vs golden on passage `{label}`"
        );
    }
}
