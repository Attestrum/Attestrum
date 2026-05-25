//! Sprint 2 E9 local determinism gate.
//!
//! Mirrors `crates/attestrum-cas/examples/sprint-2-corpus.rs` exactly,
//! runs the computation twice in-process, and asserts byte-identical
//! output AND the exact 65-byte (64 hex + 1 newline) shape that the
//! determinism CI matrix's `compare` job will diff across four
//! targets.
//!
//! In-process determinism is **necessary but not sufficient** for
//! cross-target determinism. This test catches in-process
//! non-determinism sources (HashMap iteration order, env-dependent
//! state, time-seeded RNG, mutable global state). Cross-platform
//! determinism — different rustc backends, different libc/musl,
//! different word orderings — is the four-target matrix's job
//! (`.github/workflows/determinism.yml`).
//!
//! **Convention**: this file's compute logic is a verbatim copy of
//! the example's body. If you change the example (corpus shape, hash
//! algorithm, sort, output format), update this file in the same
//! commit so the local gate keeps mirroring what CI will see.

use std::io::Write;

use attestrum_cas::stream_hash;
use attestrum_core::hex;
use attestrum_merkle::merkle_root;

const CORPUS_SIZE: usize = 1000;

fn compute_corpus_root_output() -> Vec<u8> {
    let documents: Vec<Vec<u8>> = (0..CORPUS_SIZE)
        .map(|i| format!("annex-sprint-2-doc-{i:04}").into_bytes())
        .collect();

    let mut digests: Vec<[u8; 32]> = documents
        .iter()
        .map(|doc| {
            stream_hash(doc.as_slice())
                .expect("stream_hash over an in-memory slice cannot fail")
                .blake3
        })
        .collect();
    digests.sort_unstable();

    let root = merkle_root(&digests);
    let hex_root = hex::encode_32(&root);

    let mut out = Vec::with_capacity(65);
    out.write_all(hex_root.as_bytes())
        .expect("write hex to Vec");
    out.write_all(b"\n").expect("write newline to Vec");
    out
}

#[test]
fn sprint_2_corpus_root_is_in_process_deterministic() {
    let first = compute_corpus_root_output();
    let second = compute_corpus_root_output();
    assert_eq!(
        first, second,
        "two in-process computations of the Sprint 2 corpus root diverged \
         — likely in-process non-determinism (HashMap iteration order, env, \
         clock, unseeded RNG, or mutable global state)"
    );
}

#[test]
fn sprint_2_corpus_root_output_shape_matches_ci_compare_expectations() {
    let out = compute_corpus_root_output();
    assert_eq!(
        out.len(),
        65,
        "expected exactly 64 hex chars + 1 newline = 65 bytes; got {} bytes",
        out.len()
    );
    assert!(
        out.ends_with(b"\n"),
        "expected trailing `\\n` terminator; the CI compare job diffs byte-for-byte and a missing newline would still pass cmp but break any tool that expects line-terminated input"
    );
    assert!(
        out[..64]
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "expected 64 lowercase hex chars [0-9a-f]; got: {:?}",
        std::str::from_utf8(&out[..64]).unwrap_or("<non-utf8>")
    );
}
