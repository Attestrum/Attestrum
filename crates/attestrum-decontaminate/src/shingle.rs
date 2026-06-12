//! Word n-gram shingling over already-normalized text, hashed with BLAKE3.
//!
//! Used for the `exact` (13-gram) and `contained` (5-gram set membership)
//! signals — both need raw shingle *sets*, which the PROTECTED MinHash kernel
//! (`attestrum_text_minhash::minhash::compute`) does not expose. The `near`
//! signal does not use this module; it uses the kernel's signature directly.
//!
//! BLAKE3 is the workspace's hash everywhere, so this adds no `xxhash`
//! dependency. The shingle hash is the first 8 bytes of the BLAKE3 digest read
//! as a little-endian `u64`; 64-bit collision probability is negligible for
//! set-membership counting.

/// Hash every word n-gram of `normalized` (already passed through
/// [`attestrum_text_minhash::normalize_text`]). Words inside a shingle are
/// joined with a `\x1F` unit separator before hashing so word boundaries are
/// unambiguous — `"ab c"` and `"a bc"` cannot collide structurally.
///
/// Returns a deduplicated, ascending-sorted vector (a set in deterministic
/// order). Text shorter than `n` words yields an empty set.
pub fn shingle_hashes(normalized: &str, n: usize) -> Vec<u64> {
    let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    if words.len() < n {
        return Vec::new();
    }
    let mut hashes: Vec<u64> = words
        .windows(n)
        .map(|w| {
            let joined = w.join("\x1F");
            let digest = blake3::hash(joined.as_bytes());
            let first8: [u8; 8] = digest.as_bytes()[..8]
                .try_into()
                .expect("BLAKE3 digest is 32 bytes; first 8 always present");
            u64::from_le_bytes(first8)
        })
        .collect();
    hashes.sort_unstable();
    hashes.dedup();
    hashes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match_distinct_window_count() {
        // 7 words, n=5 -> 3 windows, all distinct.
        let t = "one two three four five six seven";
        assert_eq!(shingle_hashes(t, 5).len(), 3);
    }

    #[test]
    fn short_text_yields_empty() {
        assert!(shingle_hashes("only four words here", 5).is_empty());
        assert!(shingle_hashes("", 5).is_empty());
    }

    #[test]
    fn exact_length_yields_one() {
        assert_eq!(shingle_hashes("a b c d e", 5).len(), 1);
    }

    #[test]
    fn repeated_windows_dedupe() {
        // "a b a b a b" with n=2: windows ab, ba, ab, ba, ab -> 2 distinct.
        assert_eq!(shingle_hashes("a b a b a b", 2).len(), 2);
    }

    #[test]
    fn boundary_separator_prevents_structural_collisions() {
        assert_ne!(shingle_hashes("ab c", 2), shingle_hashes("a bc", 2));
    }

    #[test]
    fn deterministic_across_calls() {
        let t = "the quick brown fox jumps over the lazy dog";
        assert_eq!(shingle_hashes(t, 5), shingle_hashes(t, 5));
    }
}
