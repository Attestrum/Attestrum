//! Integration test for `attestrum_cas::stream_hash_path` — confirms that
//! hashing a file from disk yields the same `StreamHash` as hashing
//! the same bytes from an in-memory `&[u8]`.

use std::path::PathBuf;

use attestrum_cas::{stream_hash, stream_hash_path};

#[test]
fn stream_hash_path_matches_in_memory() {
    let mut payload = vec![0u8; 100 * 1024];
    // Deterministic pseudorandom fill — xorshift64, no rand dep.
    let mut state: u64 = 0x5eed_5eed_5eed_5eed;
    let mut i = 0;
    while i < payload.len() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        let take = (payload.len() - i).min(8);
        payload[i..i + take].copy_from_slice(&bytes[..take]);
        i += take;
    }

    let mut path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    path.push("attestrum-cas-e5-stream-hash-path.bin");
    std::fs::write(&path, &payload).expect("write tempfile");

    let from_path = stream_hash_path(&path).expect("hash from path");
    let from_memory = stream_hash(payload.as_slice()).expect("hash from memory");

    assert_eq!(from_path, from_memory);
    assert_eq!(from_path.size_bytes, payload.len() as u64);

    std::fs::remove_file(&path).expect("cleanup tempfile");
}
