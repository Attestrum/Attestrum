//! PROTECTED SimHash 64 — locked algorithm parameters per the E3 landing
//! commit's `Protected-system-change:` footer (founder-approved 2026-05-25).
//!
//! See `super` for the protection rationale. The locked parameters are:
//!
//! - 5-gram **word** shingles over the already-PROTECTED-normalized text
//!   (NFC + lowercase + whitespace-collapse to single ASCII space).
//! - 64-bit SimHash via a `[i32; 64]` accumulator.
//! - Per-call key: `BLAKE3(KEY_LABEL)` where
//!   `KEY_LABEL = b"attestrum-simhash-v1"` — the label string is part of
//!   the locked spec.
//! - Per shingle: `BLAKE3_keyed(key, shingle_bytes)`; take the first
//!   8 bytes as little-endian `u64`. For bit position `i` of that hash:
//!   `+1` to `acc[i]` if the bit is 1, `-1` if it is 0. **Uniform weights**
//!   (no TF-IDF or other weighting at v1; locked).
//! - Final bit `i` of the SimHash is `1` iff `acc[i] > 0`. The tie case
//!   (`acc[i] == 0`) yields bit 0 (deterministic).
//!
//! Empty input returns `0_u64`.

const SHINGLE_SIZE: usize = 5;
const KEY_LABEL: &[u8] = b"attestrum-simhash-v1";

pub(crate) fn compute(normalized: &str) -> u64 {
    let tokens: Vec<&str> = normalized.split(' ').filter(|t| !t.is_empty()).collect();

    if tokens.is_empty() {
        return 0;
    }

    let shingles: Vec<Vec<u8>> = if tokens.len() < SHINGLE_SIZE {
        vec![tokens.join(" ").into_bytes()]
    } else {
        tokens
            .windows(SHINGLE_SIZE)
            .map(|w| w.join(" ").into_bytes())
            .collect()
    };

    let key: [u8; 32] = *blake3::hash(KEY_LABEL).as_bytes();

    let mut acc: [i32; 64] = [0; 64];
    for shingle_bytes in &shingles {
        let h = blake3::keyed_hash(&key, shingle_bytes);
        let first8: [u8; 8] = h.as_bytes()[..8]
            .try_into()
            .expect("BLAKE3 digest is 32 bytes; [..8] is always 8 bytes");
        let bits = u64::from_le_bytes(first8);
        for (i, slot) in acc.iter_mut().enumerate() {
            if (bits >> i) & 1 == 1 {
                *slot += 1;
            } else {
                *slot -= 1;
            }
        }
    }

    let mut simhash: u64 = 0;
    for (i, slot) in acc.iter().enumerate() {
        if *slot > 0 {
            simhash |= 1u64 << i;
        }
    }
    simhash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hamming(a: u64, b: u64) -> u32 {
        (a ^ b).count_ones()
    }

    #[test]
    fn compute_is_deterministic() {
        let input = "the quick brown fox jumps over the lazy dog";
        assert_eq!(compute(input), compute(input));
    }

    #[test]
    fn compute_empty_input_returns_zero() {
        assert_eq!(compute(""), 0);
    }

    #[test]
    fn compute_identical_inputs_produce_identical_hashes() {
        let a = compute("the quick brown fox jumps over the lazy dog");
        let b = compute("the quick brown fox jumps over the lazy dog");
        assert_eq!(a, b);
    }

    #[test]
    fn compute_paraphrase_pair_has_low_hamming_distance() {
        // Same 30-token paragraph pair as the MinHash high-Jaccard test.
        // ~5/26 shingles differ; per-bit accumulator drifts by at most
        // ±5 from those differing shingles, with ~21 shared shingles
        // dominating each bit's sign. Loose bound: Hamming ≤ 16 (25%
        // of 64 bits). Tight cross-target byte-equality bound at E5.
        let a = compute(
            "the quick brown fox jumps over the lazy dog in the sunny meadow at noon during the warm summer afternoon while the birds sing happily in the tall green trees",
        );
        let b = compute(
            "the quick brown fox jumped over the lazy dog in the sunny meadow at noon during the warm summer afternoon while the birds sing happily in the tall green trees",
        );
        let d = hamming(a, b);
        assert!(
            d <= 16,
            "paraphrase-pair SimHash Hamming distance was {d}; expected <= 16"
        );
    }

    #[test]
    fn compute_unrelated_documents_have_high_hamming_distance() {
        // Two structurally unrelated paragraphs share virtually no
        // shingles; each bit of the SimHash is driven by an independent
        // set of shingle-hash bits. Expected Hamming ≈ 32 (random
        // collision rate); ≥ 24 is a loose lower bound (37.5% of bits).
        let a = compute(
            "the quick brown fox jumps over the lazy dog in the sunny meadow at noon during the warm summer afternoon",
        );
        let b = compute(
            "machine learning models require careful provenance attestation pipelines and reproducible builds to satisfy audit requirements",
        );
        let d = hamming(a, b);
        assert!(
            d >= 24,
            "unrelated-document SimHash Hamming distance was {d}; expected >= 24"
        );
    }

    #[test]
    fn compute_short_input_below_shingle_threshold() {
        let h_two = compute("hello world");
        let h_two_again = compute("hello world");
        let h_three = compute("hello deterministic world");
        let h_four = compute("the lazy brown dog");
        assert_eq!(h_two, h_two_again, "short-input determinism");
        // Different short inputs MUST NOT collapse to the same hash.
        assert_ne!(h_two, h_three);
        assert_ne!(h_three, h_four);
    }
}
