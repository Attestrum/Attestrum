//! Sprint 2 E9: deterministic Merkle-root assertion across CI targets.
//!
//! Synthesizes a 1000-document deterministic corpus entirely in memory
//! (no I/O, no clocks, no env vars, no RNG), streams each document
//! through `attestrum_cas::stream_hash` to a BLAKE3 digest, sorts the
//! digests, computes the RFC 6962 Merkle root via
//! `attestrum_merkle::merkle_root`, and prints the root as 64 lowercase
//! hex characters followed by a single `\n` newline (exactly 65 bytes
//! total) to stdout.
//!
//! Run by every target in `.github/workflows/determinism.yml`. A
//! separate `compare` job in the same workflow downloads all four
//! per-target stdout captures and `cmp`s them pairwise; any byte
//! difference is a determinism regression and breaks the build. The
//! local-only mirror of this check lives at
//! `crates/attestrum-cas/tests/determinism_local.rs`.
//!
//! **If you change this file, update the local test in the same
//! commit** — the two share the corpus shape, hash algorithm, sort
//! order, and output format by convention. Drift between them means
//! the local gate stops mirroring what CI will see.

use std::io::Write;

use attestrum_cas::stream_hash;
use attestrum_core::hex;
use attestrum_merkle::merkle_root;

const CORPUS_SIZE: usize = 1000;

fn main() -> std::io::Result<()> {
    // 1. Synthesize 1000 deterministic byte strings. The exact corpus
    //    content is arbitrary — what matters is that every run of
    //    this binary on every target produces the same byte sequences.
    let documents: Vec<Vec<u8>> = (0..CORPUS_SIZE)
        .map(|i| format!("annex-sprint-2-doc-{i:04}").into_bytes())
        .collect();

    // 2. BLAKE3 + SHA-256 stream-hash each document. We discard the
    //    SHA-256 + size_bytes fields here; only the BLAKE3 digest is
    //    needed for the Merkle leaf.
    let mut digests: Vec<[u8; 32]> = documents
        .iter()
        .map(|doc| {
            stream_hash(doc.as_slice())
                .expect("stream_hash over an in-memory slice cannot fail")
                .blake3
        })
        .collect();

    // 3. Sort by BLAKE3 digest. `merkle_root` does NOT sort
    //    internally (sort order is the caller's responsibility per
    //    attestrum-merkle's public contract). Sorting here gives
    //    permutation-invariance across runs that happen to produce
    //    digests in different orders.
    digests.sort_unstable();

    // 4. RFC 6962 binary Merkle root over the sorted digests.
    let root = merkle_root(&digests);

    // 5. Emit exactly 65 bytes: 64 hex chars + one `\n`. Use
    //    `write_all` directly so no platform line-ending translation
    //    or println formatting can ever inject extra bytes.
    let hex_root = hex::encode_32(&root);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(hex_root.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}
