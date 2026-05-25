//! `attestrum-cas` — content-addressed storage for Attestrum.
//!
//! Sprint 2 E5 ships the streaming BLAKE3 + SHA-256 hasher. Bytes from
//! an `io::Read` source are teed into both hashers in a single pass
//! using an 8 KiB scratch buffer, so a 100 GB document never lands in
//! RAM. The contract is pinned by `docs/diagrams/sprint-2/hash-stream.md`.
//!
//! Sprint 2 E6 adds the [`store::CasStore`] atomic write path under
//! `.attestrum/cas/blake3/<ab>/<cd>/<hex>.bin` with staging via
//! `.attestrum/tmp/`, per `PATH-A-BRIEF.md` §1.9. The on-disk layout is
//! PROTECTED per CLAUDE.md §4 once shipped.

pub mod store;

pub use store::CasStore;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::Digest;

/// Result of a single-pass stream hash over an `io::Read` source.
///
/// Holds the BLAKE3 digest (Attestrum's primary content address), the
/// SHA-256 digest (kept for Sigstore / in-toto interop per
/// `BUILD-PLAN.md` §3.4), and the total byte count consumed from the
/// reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamHash {
    pub blake3: [u8; 32],
    pub sha256: [u8; 32],
    pub size_bytes: u64,
}

/// Scratch buffer size for the streaming hasher. Matches the BLAKE3
/// documentation recommendation; large enough to amortise per-chunk
/// hasher call overhead, small enough to comfortably fit in L1 cache.
const STREAM_BUFFER_BYTES: usize = 8 * 1024;

/// Stream `reader` through BLAKE3 and SHA-256 in a single pass.
///
/// Reads in 8 KiB chunks, tees each chunk into both hashers, and
/// accumulates the total byte count. Never holds the full input in
/// memory.
///
/// Returns an `io::Error` if the reader fails mid-stream; the partial
/// hasher state is discarded.
pub fn stream_hash<R: Read>(mut reader: R) -> io::Result<StreamHash> {
    let mut blake3_hasher = blake3::Hasher::new();
    let mut sha256_hasher = sha2::Sha256::new();
    let mut size_bytes: u64 = 0;
    let mut buf = [0u8; STREAM_BUFFER_BYTES];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        blake3_hasher.update(chunk);
        sha256_hasher.update(chunk);
        size_bytes += n as u64;
    }
    let blake3_digest: [u8; 32] = *blake3_hasher.finalize().as_bytes();
    let sha256_digest: [u8; 32] = sha256_hasher.finalize().into();
    Ok(StreamHash {
        blake3: blake3_digest,
        sha256: sha256_digest,
        size_bytes,
    })
}

/// Open `path` and stream-hash its contents. Convenience wrapper
/// around `stream_hash` for the common file-on-disk case.
pub fn stream_hash_path(path: &Path) -> io::Result<StreamHash> {
    let file = File::open(path)?;
    stream_hash(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    // BLAKE3 of the empty string. Canonical reference vector from the
    // BLAKE3 specification.
    const BLAKE3_EMPTY_HEX: &str =
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";
    // SHA-256 of the empty string. NIST CAVS / FIPS 180-4 test vector.
    const SHA256_EMPTY_HEX: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    // SHA-256 of "abc". FIPS 180-4 Appendix B.1.
    const SHA256_ABC_HEX: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    // BLAKE3 of "abc" as computed by the BLAKE3 reference implementation
    // (no official BLAKE3 spec vector for the "abc" input exists; the
    // spec's test vectors use a 0,1,2,... byte pattern). Pinned here so
    // a regression in the blake3 crate or in stream_hash gets caught
    // by a hardcoded hex literal rather than a self-referential check.
    const BLAKE3_ABC_HEX: &str = "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";

    fn hex_to_32(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64, "hex string must be 64 chars");
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            let pair = &hex[i * 2..i * 2 + 2];
            *byte = u8::from_str_radix(pair, 16).expect("valid hex");
        }
        out
    }

    /// xorshift64 — deterministic pseudorandom stream for test data.
    /// No external RNG dep needed.
    fn xorshift_fill(buf: &mut [u8], seed: u64) {
        let mut state = seed;
        let mut i = 0;
        while i < buf.len() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bytes = state.to_le_bytes();
            let take = (buf.len() - i).min(8);
            buf[i..i + take].copy_from_slice(&bytes[..take]);
            i += take;
        }
    }

    #[test]
    fn empty_input_known_vectors() {
        let result = stream_hash(&[][..]).expect("hash empty input");
        assert_eq!(result.blake3, hex_to_32(BLAKE3_EMPTY_HEX));
        assert_eq!(result.sha256, hex_to_32(SHA256_EMPTY_HEX));
        assert_eq!(result.size_bytes, 0);
    }

    #[test]
    fn single_byte_matches_oneshot() {
        let input = [0u8];
        let streamed = stream_hash(&input[..]).expect("hash single byte");
        let blake3_oneshot: [u8; 32] = *blake3::hash(&input).as_bytes();
        let sha256_oneshot: [u8; 32] = sha2::Sha256::digest(input).into();
        assert_eq!(streamed.blake3, blake3_oneshot);
        assert_eq!(streamed.sha256, sha256_oneshot);
        assert_eq!(streamed.size_bytes, 1);
    }

    #[test]
    fn exactly_one_buffer_matches_oneshot() {
        // Fixed pattern, exactly STREAM_BUFFER_BYTES long: exercises
        // the loop's first-iteration full-buffer path with no second
        // iteration.
        let input = vec![0xa5u8; STREAM_BUFFER_BYTES];
        let streamed = stream_hash(input.as_slice()).expect("hash one buffer");
        let blake3_oneshot: [u8; 32] = *blake3::hash(&input).as_bytes();
        let sha256_oneshot: [u8; 32] = sha2::Sha256::digest(&input).into();
        assert_eq!(streamed.blake3, blake3_oneshot);
        assert_eq!(streamed.sha256, sha256_oneshot);
        assert_eq!(streamed.size_bytes, STREAM_BUFFER_BYTES as u64);
    }

    #[test]
    fn multi_buffer_pseudorandom_matches_oneshot() {
        // 64 KiB = 8 × STREAM_BUFFER_BYTES: exercises 8 loop iterations
        // plus the EOF read.
        let mut input = vec![0u8; 64 * 1024];
        xorshift_fill(&mut input, 0xdeadbeef);
        let streamed = stream_hash(input.as_slice()).expect("hash multi buffer");
        let blake3_oneshot: [u8; 32] = *blake3::hash(&input).as_bytes();
        let sha256_oneshot: [u8; 32] = sha2::Sha256::digest(&input).into();
        assert_eq!(streamed.blake3, blake3_oneshot);
        assert_eq!(streamed.sha256, sha256_oneshot);
        assert_eq!(streamed.size_bytes, input.len() as u64);
    }

    #[test]
    fn abc_known_vectors() {
        let result = stream_hash(&b"abc"[..]).expect("hash abc");
        assert_eq!(result.blake3, hex_to_32(BLAKE3_ABC_HEX));
        assert_eq!(result.sha256, hex_to_32(SHA256_ABC_HEX));
        assert_eq!(result.size_bytes, 3);
    }

    #[test]
    fn reader_error_propagates() {
        struct FailAfter {
            bytes: Vec<u8>,
            position: usize,
            fail_after: usize,
        }
        impl Read for FailAfter {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.position >= self.fail_after {
                    return Err(io::Error::other("simulated mid-stream failure"));
                }
                let remaining = &self.bytes[self.position..];
                let n = remaining.len().min(buf.len());
                buf[..n].copy_from_slice(&remaining[..n]);
                self.position += n;
                Ok(n)
            }
        }
        let reader = FailAfter {
            bytes: vec![0u8; 16 * 1024],
            position: 0,
            fail_after: 4 * 1024,
        };
        let err = stream_hash(reader).expect_err("expected mid-stream error");
        assert_eq!(err.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn deterministic_across_calls() {
        let mut input = vec![0u8; 12_345];
        xorshift_fill(&mut input, 0xc0ffee);
        let first = stream_hash(input.as_slice()).expect("hash first");
        let second = stream_hash(input.as_slice()).expect("hash second");
        assert_eq!(first, second);
    }
}
