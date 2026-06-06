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

// ============================================================================
// Hamming pigeonhole banding (image perceptual + ISCC).
//
// For a Hamming threshold `k`, splitting a hash into ≥ k+1 exact-match bands
// guarantees that any two hashes within `k` bits agree exactly on at least one
// band (pigeonhole) — so candidate gathering has EXACT recall (zero false
// negatives) for distance ≤ k, unlike MinHash LSH's probabilistic recall.
// ============================================================================

/// Locked perceptual Hamming threshold (`min(phash, blockhash) ≤ 6`), mirroring
/// `attestrum-prove`'s `FUZZY_THRESHOLD_PERCEPTUAL_HAMMING`.
pub const PERCEPTUAL_THRESHOLD: u32 = 6;
/// Exact-match bands per 64-bit hash. 8 ≥ `PERCEPTUAL_THRESHOLD + 1`, so the
/// pigeonhole recall guarantee holds for distance ≤ 7 (covers the ≤6 gate).
pub const HAMMING64_BANDS: u16 = 8;
/// Bits per band (`64 / HAMMING64_BANDS`).
pub const HAMMING64_BAND_BITS: u16 = 8;

/// Decode a 16-char lowercase-hex perceptual hash (pHash / blockhash) into a
/// `u64` (big-endian). The one blessed conversion shared by the builder and the
/// prove fast-path: Hamming distance is representation-invariant as long as both
/// sides pack bits identically.
pub fn perceptual_hex_to_u64(s: &str) -> Option<u64> {
    if s.len() != 16 {
        return None;
    }
    let mut bytes = [0u8; 8];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(u64::from_be_bytes(bytes))
}

/// Band one 64-bit hash into `HAMMING64_BANDS` exact-match bands, offsetting the
/// band ids by `band_id_offset` so two hashes (pHash + blockhash) can share one
/// bucket map without colliding band ids.
fn band_hamming64(hash: u64, band_id_offset: u16) -> Vec<(u16, u64)> {
    let mut keys = Vec::with_capacity(HAMMING64_BANDS as usize);
    for b in 0..HAMMING64_BANDS {
        let shift = (b * HAMMING64_BAND_BITS) as u32;
        let band_bits = (hash >> shift) & 0xFF;
        keys.push((band_id_offset + b, band_bits));
    }
    keys
}

/// Band a perceptual signature `(phash, blockhash)`. pHash occupies band ids
/// `0..HAMMING64_BANDS`, blockhash `HAMMING64_BANDS..2*HAMMING64_BANDS`. A query
/// is a candidate if EITHER hash collides in a band — the union mirrors prove's
/// `min(phash_dist, blockhash_dist) ≤ 6` so the indexed candidate set is a
/// superset of the exhaustive match set.
pub fn band_perceptual(phash: u64, blockhash: u64) -> Vec<(u16, u64)> {
    let mut keys = band_hamming64(phash, 0);
    keys.extend(band_hamming64(blockhash, HAMMING64_BANDS));
    keys
}

/// Total band ids used by a perceptual sub-index (pHash + blockhash).
pub const PERCEPTUAL_BANDS: u16 = 2 * HAMMING64_BANDS;

/// Locked ISCC composite-distance threshold (`iscc_composite_distance ≤ 4`),
/// mirroring `attestrum-prove`'s `FUZZY_THRESHOLD_ISCC_DISTANCE`. The distance
/// is a single global Hamming over the decoded composite body bytes
/// (`attestrum-prove::iscc_composite_distance`), so global pigeonhole banding
/// applies — no per-component split is needed (verified against
/// `iscc_composite_distance`'s `a_body`/`b_body` global XOR sum).
pub const ISCC_THRESHOLD: u32 = 4;
/// Exact-match bands over the ISCC composite body (`≥ ISCC_THRESHOLD + 1`).
pub const ISCC_BANDS: u16 = 5;

/// Band a decoded ISCC composite body into `ISCC_BANDS` exact-match bands over
/// contiguous byte ranges. `band_hash` = first 8 LE bytes of BLAKE3 over the
/// band's bytes. A differing bit lands in exactly one band, so ≤4 differing
/// bits touch ≤4 of the 5 bands → ≥1 band is byte-identical → collision
/// (pigeonhole, exact recall for distance ≤ 4).
pub fn band_iscc(body: &[u8]) -> Vec<(u16, u64)> {
    let n = body.len();
    let mut keys = Vec::with_capacity(ISCC_BANDS as usize);
    for b in 0..ISCC_BANDS {
        let start = (b as usize * n) / ISCC_BANDS as usize;
        let end = ((b as usize + 1) * n) / ISCC_BANDS as usize;
        let h = blake3::hash(&body[start..end]);
        let band_hash = u64::from_le_bytes(h.as_bytes()[..8].try_into().expect("8 bytes"));
        keys.push((b, band_hash));
    }
    keys
}

/// Pack the ISCC composite body bytes into little-endian `u64`s (zero-padding a
/// final partial chunk). The one blessed packing shared by the builder (stored
/// signature) and the prove fast-path (query body) — Hamming over equal-length
/// packed signatures equals Hamming over the original body bytes because the
/// shared zero padding never contributes set bits.
pub fn pack_iscc_body(body: &[u8]) -> Vec<u64> {
    body.chunks(8)
        .map(|c| {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            u64::from_le_bytes(b)
        })
        .collect()
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

    #[test]
    fn perceptual_hex_decodes_to_u64() {
        assert_eq!(perceptual_hex_to_u64("0000000000000000"), Some(0));
        assert_eq!(perceptual_hex_to_u64("00000000000000ff"), Some(0xff));
        assert_eq!(
            perceptual_hex_to_u64("ff00000000000000"),
            Some(0xff00_0000_0000_0000)
        );
        assert_eq!(perceptual_hex_to_u64("zz00000000000000"), None);
        assert_eq!(perceptual_hex_to_u64("00"), None);
    }

    #[test]
    fn perceptual_bands_phash_and_blockhash_disjointly() {
        let keys = band_perceptual(0xdead_beef_0000_1111, 0x0123_4567_89ab_cdef);
        assert_eq!(keys.len(), PERCEPTUAL_BANDS as usize);
        // phash bands 0..8, blockhash bands 8..16
        for (i, (band_id, _)) in keys.iter().enumerate() {
            assert_eq!(*band_id, i as u16);
        }
    }

    /// Pigeonhole guarantee: two hashes within PERCEPTUAL_THRESHOLD bits must
    /// share at least one identical band.
    #[test]
    fn within_threshold_hashes_share_a_band() {
        let a: u64 = 0xa5a5_a5a5_a5a5_a5a5;
        // flip exactly 6 bits spread across distinct bytes (one per band, worst case)
        let b = a ^ 0x0101_0101_0101_0000; // 6 set bits across 6 of the 8 bands
        assert_eq!((a ^ b).count_ones(), PERCEPTUAL_THRESHOLD);
        let ka = band_hamming64(a, 0);
        let kb = band_hamming64(b, 0);
        let shared = ka.iter().filter(|k| kb.contains(k)).count();
        assert!(shared >= 1, "≤6-bit-different hashes must share ≥1 band");
    }

    #[test]
    fn iscc_bands_count_and_determinism() {
        let body: Vec<u8> = (0..24u8).collect();
        let k = band_iscc(&body);
        assert_eq!(k.len(), ISCC_BANDS as usize);
        assert_eq!(band_iscc(&body), band_iscc(&body));
    }

    /// Pigeonhole: ≤4 differing bits across the body touch ≤4 of 5 bands, so the
    /// two bodies must share ≥1 identical band.
    #[test]
    fn iscc_within_threshold_bodies_share_a_band() {
        let a: Vec<u8> = (0..40u8).collect(); // 40 bytes → 8 per band
        let mut b = a.clone();
        // flip 4 bits, each in a different band (bytes 0, 8, 16, 24)
        b[0] ^= 0x01;
        b[8] ^= 0x01;
        b[16] ^= 0x01;
        b[24] ^= 0x01;
        let total: u32 = a.iter().zip(&b).map(|(x, y)| (x ^ y).count_ones()).sum();
        assert_eq!(total, ISCC_THRESHOLD);
        let ka = band_iscc(&a);
        let kb = band_iscc(&b);
        assert!(ka.iter().filter(|k| kb.contains(k)).count() >= 1);
    }

    #[test]
    fn pack_iscc_body_zero_pads_last_chunk() {
        assert_eq!(pack_iscc_body(&[1, 0, 0, 0, 0, 0, 0, 0]), vec![1u64]);
        // 9 bytes → 2 u64s, second zero-padded
        let packed = pack_iscc_body(&[0, 0, 0, 0, 0, 0, 0, 0, 0xff]);
        assert_eq!(packed, vec![0u64, 0xffu64]);
        // Hamming over packed == Hamming over bytes (shared padding cancels)
        let a = pack_iscc_body(&[0xff, 0, 0, 0, 0]);
        let b = pack_iscc_body(&[0x0f, 0, 0, 0, 0]);
        let dist: u32 = a.iter().zip(&b).map(|(x, y)| (x ^ y).count_ones()).sum();
        assert_eq!(dist, 4); // 0xff ^ 0x0f = 0xf0 → 4 bits
    }
}
