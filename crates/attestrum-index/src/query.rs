//! LSH banding — the bridge between a fuzzy signature and the sidecar's
//! `(band_id, band_hash)` bucket keys. Shared by the builder (which inserts a
//! leaf's signature into every band bucket) and the querier (which gathers
//! candidates from the query's band buckets).
//!
//! MinHash text uses band-of-perms LSH (this commit). Image perceptual and
//! ISCC use Hamming pigeonhole banding (later commits). In every case the band
//! keys only *narrow candidates* — the exact recheck in `attestrum-prove`
//! (`minhash_jaccard_ppm` ≥ 850000, etc.) is the unchanged correctness gate.

/// Locked MinHash signature width — mirrors `attestrum-fingerprint`'s
/// `TextFingerprint::minhash` length (128 BLAKE3-keyed permutations).
pub const MINHASH_PERMS: usize = 128;
/// MinHash LSH bands. `MINHASH_BANDS * MINHASH_ROWS == MINHASH_PERMS`.
///
/// **Benchmark-tunable, NOT §4-locked.** The locked exact Jaccard recheck is
/// the correctness gate; banding only narrows candidates, so changing these is
/// reversible and never alters an emitted proof. 32×4 places the LSH S-curve
/// knee `(1/32)^(1/4) ≈ 0.42` well below the locked 0.85 threshold, so a true
/// ≥0.85 match collides in ≥1 band with probability ≈ 0.99999 (miss-rate
/// ~1e-5) while still pruning the unrelated tail.
pub const MINHASH_BANDS: u16 = 32;
/// MinHash LSH rows per band.
pub const MINHASH_ROWS: usize = 4;

/// Band a 128-perm MinHash signature into `MINHASH_BANDS` `(band_id, band_hash)`
/// keys. `band_hash` is the first 8 little-endian bytes of BLAKE3 over the
/// band's `MINHASH_ROWS` little-endian perms — the one blessed
/// `from_le_bytes(first8)` convention shared with `attestrum-fingerprint`'s
/// MinHash. Two signatures share a band key iff their perms in that band are
/// all equal, which is exactly the MinHash-LSH collision condition.
pub fn band_minhash(sig: &[u64]) -> Vec<(u16, u64)> {
    debug_assert_eq!(
        sig.len(),
        MINHASH_PERMS,
        "MinHash signature must be exactly 128 perms"
    );
    let mut keys = Vec::with_capacity(MINHASH_BANDS as usize);
    for b in 0..MINHASH_BANDS {
        let start = b as usize * MINHASH_ROWS;
        let mut buf = [0u8; MINHASH_ROWS * 8];
        for (i, v) in sig[start..start + MINHASH_ROWS].iter().enumerate() {
            buf[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
        }
        let h = blake3::hash(&buf);
        let band_hash = u64::from_le_bytes(h.as_bytes()[..8].try_into().expect("8 bytes"));
        keys.push((b, band_hash));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig_of(seed: u64) -> Vec<u64> {
        (0..MINHASH_PERMS as u64)
            .map(|i| seed.wrapping_add(i))
            .collect()
    }

    #[test]
    fn produces_one_key_per_band() {
        let keys = band_minhash(&sig_of(0));
        assert_eq!(keys.len(), MINHASH_BANDS as usize);
        // band ids are 0..MINHASH_BANDS in order
        for (i, (band_id, _)) in keys.iter().enumerate() {
            assert_eq!(*band_id, i as u16);
        }
    }

    #[test]
    fn deterministic_same_input_same_keys() {
        assert_eq!(band_minhash(&sig_of(42)), band_minhash(&sig_of(42)));
    }

    #[test]
    fn identical_signatures_share_all_bands() {
        let a = band_minhash(&sig_of(7));
        let b = band_minhash(&sig_of(7));
        assert_eq!(a, b);
    }

    #[test]
    fn one_band_differs_when_only_that_band_changes() {
        let base = sig_of(0);
        let mut mutated = base.clone();
        mutated[0] = 9999; // perturb only band 0 (rows 0..4)
        let kb = band_minhash(&base);
        let km = band_minhash(&mutated);
        assert_ne!(kb[0], km[0], "band 0 hash must change");
        for i in 1..MINHASH_BANDS as usize {
            assert_eq!(kb[i], km[i], "bands 1.. must be unchanged");
        }
    }
}
