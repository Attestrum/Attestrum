//! PROTECTED MinHash 128 — locked algorithm parameters per the E3 landing
//! commit's `Protected-system-change:` footer (founder-approved 2026-05-25).
//!
//! See `super` for the protection rationale. The locked parameters are:
//!
//! - 5-gram **word** shingles over the already-PROTECTED-normalized text
//!   (NFC + lowercase + whitespace-collapse to single ASCII space).
//! - 128 permutations.
//! - Each permutation `i` derives a 32-byte BLAKE3 keyed-mode key from
//!   `BLAKE3(KEY_PREFIX || u64_le_bytes(i))` where
//!   `KEY_PREFIX = b"attestrum-minhash-v1-perm-"` — the prefix string is
//!   part of the locked spec.
//! - Per shingle: `BLAKE3_keyed(key_i, shingle_bytes)`; take the first
//!   8 bytes as little-endian `u64`. Keep the MIN across all shingles.
//!
//! Empty input returns `vec![u64::MAX; 128]` — caller-side Jaccard against
//! any non-empty document yields 0 (the expected "no overlap" answer).

const PERMUTATIONS: usize = 128;
const SHINGLE_SIZE: usize = 5;
const KEY_PREFIX: &[u8] = b"attestrum-minhash-v1-perm-";

pub(crate) fn compute(normalized: &str) -> Vec<u64> {
    // Tokenize on the single-ASCII-space delimiter guaranteed by
    // `normalize_text`. The non-empty filter collapses the degenerate
    // `"".split(' ') == [""]` case.
    let tokens: Vec<&str> = normalized.split(' ').filter(|t| !t.is_empty()).collect();

    if tokens.is_empty() {
        return vec![u64::MAX; PERMUTATIONS];
    }

    // For inputs of <5 tokens the fallback is a single shingle containing
    // the full token list (locked behavior per E3 plan). Otherwise:
    // overlapping 5-gram word windows.
    let shingles: Vec<Vec<u8>> = if tokens.len() < SHINGLE_SIZE {
        vec![tokens.join(" ").into_bytes()]
    } else {
        tokens
            .windows(SHINGLE_SIZE)
            .map(|w| w.join(" ").into_bytes())
            .collect()
    };

    let mut out = Vec::with_capacity(PERMUTATIONS);
    let mut key_input = Vec::with_capacity(KEY_PREFIX.len() + 8);
    for i in 0..PERMUTATIONS {
        key_input.clear();
        key_input.extend_from_slice(KEY_PREFIX);
        key_input.extend_from_slice(&(i as u64).to_le_bytes());
        let key: [u8; 32] = *blake3::hash(&key_input).as_bytes();

        let mut min_val = u64::MAX;
        for shingle_bytes in &shingles {
            let h = blake3::keyed_hash(&key, shingle_bytes);
            let first8: [u8; 8] = h.as_bytes()[..8]
                .try_into()
                .expect("BLAKE3 digest is 32 bytes; [..8] is always 8 bytes");
            let val = u64::from_le_bytes(first8);
            if val < min_val {
                min_val = val;
            }
        }
        out.push(min_val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_returns_128_entries() {
        let h = compute("hello world from the deterministic fingerprint pipeline");
        assert_eq!(h.len(), 128);
    }

    #[test]
    fn compute_is_deterministic() {
        let input = "the quick brown fox jumps over the lazy dog";
        let a = compute(input);
        let b = compute(input);
        assert_eq!(a, b);
    }

    #[test]
    fn compute_empty_input_returns_max_filled() {
        assert_eq!(compute(""), vec![u64::MAX; 128]);
    }

    #[test]
    fn compute_identical_inputs_produce_identical_hashes() {
        let a = compute("the quick brown fox jumps over the lazy dog");
        let b = compute("the quick brown fox jumps over the lazy dog");
        assert_eq!(a, b);
    }

    #[test]
    fn compute_paraphrase_pair_has_high_jaccard() {
        // 30-token paragraph; the change (jumps → jumped) appears in 5 of
        // the 26 overlapping 5-gram shingles. True Jaccard ≈ 21/31 ≈ 0.677.
        // Expected MinHash matching-positions ratio ≈ 0.68 ± ~0.04 std dev
        // at 128 permutations; ≥ 0.5 is a loose lower bound (~4σ below the
        // mean). Tight cross-target byte-equality bound lands at E5.
        let a = compute(
            "the quick brown fox jumps over the lazy dog in the sunny meadow at noon during the warm summer afternoon while the birds sing happily in the tall green trees",
        );
        let b = compute(
            "the quick brown fox jumped over the lazy dog in the sunny meadow at noon during the warm summer afternoon while the birds sing happily in the tall green trees",
        );
        let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
        let jaccard = matches as f64 / 128.0;
        assert!(
            jaccard >= 0.5,
            "paraphrase pair Jaccard was {jaccard}; expected >= 0.5 (matches={matches}/128)"
        );
    }

    #[test]
    fn compute_unrelated_documents_have_low_jaccard() {
        let a = compute(
            "the quick brown fox jumps over the lazy dog in the sunny meadow at noon during the warm summer afternoon",
        );
        let b = compute(
            "machine learning models require careful provenance attestation pipelines and reproducible builds to satisfy audit requirements",
        );
        let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
        let jaccard = matches as f64 / 128.0;
        assert!(
            jaccard <= 0.10,
            "unrelated-document Jaccard was {jaccard}; expected <= 0.10 (matches={matches}/128)"
        );
    }

    #[test]
    fn compute_short_input_below_shingle_threshold() {
        let h_two = compute("hello world");
        let h_three = compute("hello deterministic world");
        let h_four = compute("the lazy brown dog");
        for h in [&h_two, &h_three, &h_four] {
            assert_eq!(h.len(), 128, "short-input MinHash must still be length 128");
        }
        let h_two_again = compute("hello world");
        assert_eq!(h_two, h_two_again, "short-input determinism");
        // Different short inputs must NOT collapse to the same Vec.
        assert_ne!(h_two, h_three);
        assert_ne!(h_three, h_four);
    }
}
