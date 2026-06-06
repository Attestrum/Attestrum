//! On-disk format for the fuzzy-lookup sidecar index (v1).
//!
//! Raw little-endian binary, no codec — matches the `to_le_bytes` /
//! `from_le_bytes` convention in `attestrum-fingerprint`'s MinHash and is the
//! §7-cleanest encoding for fixed-width `u64` arrays (no Parquet
//! page/dictionary drift, no float, no map-iteration-order nondeterminism).
//! Buckets live in a [`BTreeMap`] and are written in sorted `(band_id,
//! band_hash)` order with ascending members, so a rebuild from a fixed manifest
//! is byte-identical. A trailing BLAKE3 digest (`TRAILER_HASH`) over all
//! preceding bytes catches partial writes and makes the golden a one-line
//! assertion.
//!
//! This module is **kind-agnostic**: it stores opaque `u64` signatures plus a
//! band→members bucket map. The kind-specific banding math (MinHash 32×4 vs
//! Hamming pigeonhole) lives in the build/query modules. The layout is the
//! `erDiagram` in `docs/diagrams/index/sidecar-format.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

use crate::error::IndexError;

/// File magic — first 8 bytes of every sidecar.
pub const MAGIC: [u8; 8] = *b"ATSTRMIX";
/// On-disk format version understood by this build.
pub const FORMAT_VER: u16 = 1;

/// Which fuzzy modality a sidecar indexes. One file per kind under
/// `.attestrum/index/<subdir>/v1.idx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubIndexKind {
    /// Text MinHash containment (`index/minhash/`).
    Minhash,
    /// Image perceptual pHash + blockhash (`index/perceptual/`).
    Perceptual,
    /// ISCC composite (`index/iscc/`).
    Iscc,
}

impl SubIndexKind {
    fn as_u16(self) -> u16 {
        match self {
            SubIndexKind::Minhash => 1,
            SubIndexKind::Perceptual => 2,
            SubIndexKind::Iscc => 3,
        }
    }

    fn from_u16(v: u16) -> Result<Self, IndexError> {
        match v {
            1 => Ok(SubIndexKind::Minhash),
            2 => Ok(SubIndexKind::Perceptual),
            3 => Ok(SubIndexKind::Iscc),
            other => Err(IndexError::UnknownKind(other)),
        }
    }

    /// The `.attestrum/index/<subdir>/` directory name for this kind.
    pub fn subdir(self) -> &'static str {
        match self {
            SubIndexKind::Minhash => "minhash",
            SubIndexKind::Perceptual => "perceptual",
            SubIndexKind::Iscc => "iscc",
        }
    }
}

/// One indexed leaf: its manifest row index and its fuzzy signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigEntry {
    /// Manifest row index this signature belongs to.
    pub row: u64,
    /// The raw signature — `SIG_WIDTH` `u64`s (128 for MinHash, 2 for
    /// perceptual pHash+blockhash, N for an ISCC composite).
    pub sig: Vec<u64>,
}

/// A loaded (or freshly built) fuzzy-lookup sidecar index.
///
/// Kind-agnostic container: opaque signatures + a band-bucket map + the corpus
/// binding root. Construct with [`FuzzyIndex::from_parts`], serialize with
/// [`FuzzyIndex::to_bytes`] / [`FuzzyIndex::write_to_path`], load with
/// [`FuzzyIndex::from_bytes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyIndex {
    kind: SubIndexKind,
    binding_root: [u8; 32],
    sig_width: u16,
    bands: u16,
    rows_or_bits: u16,
    /// Signatures sorted ascending by `row` (enables binary-search lookup).
    signatures: Vec<SigEntry>,
    /// `(band_id, band_hash)` → ascending member rows. Sorted iteration order
    /// is the determinism guarantee.
    buckets: BTreeMap<(u16, u64), Vec<u64>>,
}

impl FuzzyIndex {
    /// Assemble an index from its parts. Sorts signatures by `row` and members
    /// within each bucket, so the byte output is order-independent of the
    /// builder's insertion order. Validates that every signature has exactly
    /// `sig_width` `u64`s.
    pub fn from_parts(
        kind: SubIndexKind,
        binding_root: [u8; 32],
        sig_width: u16,
        bands: u16,
        rows_or_bits: u16,
        mut signatures: Vec<SigEntry>,
        buckets: BTreeMap<(u16, u64), Vec<u64>>,
    ) -> Result<Self, IndexError> {
        for s in &signatures {
            if s.sig.len() != sig_width as usize {
                return Err(IndexError::SignatureWidthMismatch {
                    expected: sig_width,
                    got: s.sig.len(),
                });
            }
        }
        signatures.sort_by_key(|s| s.row);
        let mut buckets = buckets;
        for members in buckets.values_mut() {
            members.sort_unstable();
            members.dedup();
        }
        Ok(Self {
            kind,
            binding_root,
            sig_width,
            bands,
            rows_or_bits,
            signatures,
            buckets,
        })
    }

    /// The kind of fuzzy signature this sidecar indexes.
    pub fn kind(&self) -> SubIndexKind {
        self.kind
    }

    /// The corpus binding root (Merkle root over `document_id` in manifest row
    /// order). The querier recomputes this from the loaded manifest and falls
    /// back to an exhaustive scan on mismatch.
    pub fn binding_root(&self) -> [u8; 32] {
        self.binding_root
    }

    /// LSH band parameters: `(bands, rows_or_bits)`.
    pub fn band_params(&self) -> (u16, u16) {
        (self.bands, self.rows_or_bits)
    }

    /// Number of indexed leaves.
    pub fn leaf_count(&self) -> usize {
        self.signatures.len()
    }

    /// The persisted signature for a manifest row, if indexed. Binary search
    /// over the row-sorted signature block — no CAS read, no re-fingerprint.
    pub fn signature(&self, row: u64) -> Option<&[u64]> {
        self.signatures
            .binary_search_by_key(&row, |s| s.row)
            .ok()
            .map(|i| self.signatures[i].sig.as_slice())
    }

    /// Candidate manifest rows for a query whose signature bands to the given
    /// `(band_id, band_hash)` keys: the de-duplicated, ascending union of all
    /// matching buckets' members. The kind-specific banding of the query lives
    /// in the query module.
    pub fn candidates(&self, band_keys: &[(u16, u64)]) -> Vec<u64> {
        let mut set: BTreeSet<u64> = BTreeSet::new();
        for key in band_keys {
            if let Some(members) = self.buckets.get(key) {
                set.extend(members.iter().copied());
            }
        }
        set.into_iter().collect()
    }

    /// Serialize to the on-disk byte layout (header + signature block + bucket
    /// block + trailer). Deterministic: identical parts → identical bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VER.to_le_bytes());
        out.extend_from_slice(&self.kind.as_u16().to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // RESERVED
        out.extend_from_slice(&self.binding_root);
        out.extend_from_slice(&self.sig_width.to_le_bytes());
        out.extend_from_slice(&self.bands.to_le_bytes());
        out.extend_from_slice(&self.rows_or_bits.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // RESERVED2
        out.extend_from_slice(&(self.signatures.len() as u64).to_le_bytes());
        for s in &self.signatures {
            out.extend_from_slice(&s.row.to_le_bytes());
            for v in &s.sig {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.buckets.len() as u64).to_le_bytes());
        for ((band_id, band_hash), members) in &self.buckets {
            out.extend_from_slice(&band_id.to_le_bytes());
            out.extend_from_slice(&band_hash.to_le_bytes());
            out.extend_from_slice(&(members.len() as u32).to_le_bytes());
            for m in members {
                out.extend_from_slice(&m.to_le_bytes());
            }
        }
        let trailer = blake3::hash(&out);
        out.extend_from_slice(trailer.as_bytes());
        out
    }

    /// Parse and validate an index from its on-disk bytes. Verifies the magic,
    /// version, kind, per-signature width, full-length availability of every
    /// field, and the trailing integrity digest.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IndexError> {
        // Trailer first: the digest must cover everything before it.
        if bytes.len() < 32 {
            return Err(IndexError::Truncated {
                offset: bytes.len(),
                needed: 32 - bytes.len(),
            });
        }
        let body_len = bytes.len() - 32;
        let expected = blake3::hash(&bytes[..body_len]);
        if expected.as_bytes() != &bytes[body_len..] {
            return Err(IndexError::TrailerMismatch);
        }

        let mut c = Cursor::new(&bytes[..body_len]);
        let magic = c.take(8)?;
        if magic != MAGIC {
            return Err(IndexError::BadMagic);
        }
        let ver = c.u16()?;
        if ver != FORMAT_VER {
            return Err(IndexError::UnsupportedVersion(ver));
        }
        let kind = SubIndexKind::from_u16(c.u16()?)?;
        let _reserved = c.u32()?;
        let mut binding_root = [0u8; 32];
        binding_root.copy_from_slice(c.take(32)?);
        let sig_width = c.u16()?;
        let bands = c.u16()?;
        let rows_or_bits = c.u16()?;
        let _reserved2 = c.u16()?;
        let leaf_count = c.u64()? as usize;

        let mut signatures = Vec::with_capacity(leaf_count);
        for _ in 0..leaf_count {
            let row = c.u64()?;
            let mut sig = Vec::with_capacity(sig_width as usize);
            for _ in 0..sig_width {
                sig.push(c.u64()?);
            }
            signatures.push(SigEntry { row, sig });
        }

        let bucket_count = c.u64()? as usize;
        let mut buckets: BTreeMap<(u16, u64), Vec<u64>> = BTreeMap::new();
        for _ in 0..bucket_count {
            let band_id = c.u16()?;
            let band_hash = c.u64()?;
            let member_cnt = c.u32()? as usize;
            let mut members = Vec::with_capacity(member_cnt);
            for _ in 0..member_cnt {
                members.push(c.u64()?);
            }
            buckets.insert((band_id, band_hash), members);
        }

        Ok(Self {
            kind,
            binding_root,
            sig_width,
            bands,
            rows_or_bits,
            signatures,
            buckets,
        })
    }

    /// Atomically write the index to `path`: stage to a sibling temp file in the
    /// same directory, `sync_all`, rename into place, and fsync the parent
    /// directory — the same durability discipline as the CAS object writer. The
    /// parent `index/<kind>/` directory is created if absent.
    pub fn write_to_path(&self, path: &Path) -> Result<(), IndexError> {
        let dir = path.parent().ok_or_else(|| {
            IndexError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "index path has no parent directory",
            ))
        })?;
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".attestrum-index-tmp.{}", std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&self.to_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all(); // best-effort parent fsync (dir fsync unsupported on some FS)
        }
        Ok(())
    }
}

/// Minimal bounds-checked little-endian byte cursor. Every read returns
/// [`IndexError::Truncated`] rather than panicking on a short buffer.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], IndexError> {
        if self.pos + n > self.buf.len() {
            return Err(IndexError::Truncated {
                offset: self.pos,
                needed: (self.pos + n) - self.buf.len(),
            });
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, IndexError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, IndexError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, IndexError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FuzzyIndex {
        let sigs = vec![
            SigEntry {
                row: 7,
                sig: vec![10, 20, 30, 40],
            },
            SigEntry {
                row: 2,
                sig: vec![1, 2, 3, 4],
            },
        ];
        let mut buckets: BTreeMap<(u16, u64), Vec<u64>> = BTreeMap::new();
        buckets.insert((0, 0xdead_beef), vec![7, 2]);
        buckets.insert((1, 0x00ff), vec![2]);
        FuzzyIndex::from_parts(SubIndexKind::Minhash, [9u8; 32], 4, 32, 4, sigs, buckets).unwrap()
    }

    #[test]
    fn roundtrip_returns_equal_index() {
        let idx = sample();
        let bytes = idx.to_bytes();
        let back = FuzzyIndex::from_bytes(&bytes).unwrap();
        assert_eq!(idx, back);
    }

    #[test]
    fn from_parts_sorts_signatures_and_members() {
        let idx = sample();
        // signatures sorted ascending by row (2 before 7)
        assert_eq!(idx.signatures[0].row, 2);
        assert_eq!(idx.signatures[1].row, 7);
        // bucket members sorted ascending (2 before 7)
        assert_eq!(idx.buckets.get(&(0, 0xdead_beef)).unwrap(), &vec![2, 7]);
    }

    #[test]
    fn byte_identical_rebuild() {
        // Same parts inserted in different order must serialize identically.
        let a = FuzzyIndex::from_parts(
            SubIndexKind::Minhash,
            [9u8; 32],
            4,
            32,
            4,
            vec![
                SigEntry {
                    row: 7,
                    sig: vec![10, 20, 30, 40],
                },
                SigEntry {
                    row: 2,
                    sig: vec![1, 2, 3, 4],
                },
            ],
            BTreeMap::from([((0, 0xdead_beef), vec![7, 2]), ((1, 0x00ff), vec![2])]),
        )
        .unwrap();
        let b = FuzzyIndex::from_parts(
            SubIndexKind::Minhash,
            [9u8; 32],
            4,
            32,
            4,
            vec![
                SigEntry {
                    row: 2,
                    sig: vec![1, 2, 3, 4],
                },
                SigEntry {
                    row: 7,
                    sig: vec![10, 20, 30, 40],
                },
            ],
            BTreeMap::from([((1, 0x00ff), vec![2]), ((0, 0xdead_beef), vec![2, 7])]),
        )
        .unwrap();
        assert_eq!(a.to_bytes(), b.to_bytes());
    }

    #[test]
    fn signature_lookup_by_row() {
        let idx = sample();
        assert_eq!(idx.signature(2), Some([1u64, 2, 3, 4].as_slice()));
        assert_eq!(idx.signature(7), Some([10u64, 20, 30, 40].as_slice()));
        assert_eq!(idx.signature(99), None);
    }

    #[test]
    fn candidates_union_dedup_sorted() {
        let idx = sample();
        let c = idx.candidates(&[(0, 0xdead_beef), (1, 0x00ff), (9, 9)]);
        assert_eq!(c, vec![2, 7]); // 2 appears in both matched buckets → deduped
    }

    #[test]
    fn write_to_path_roundtrips_on_disk() {
        let dir = std::env::temp_dir().join(format!("attestrum-index-test-{}", std::process::id()));
        let path = dir.join("minhash").join("v1.idx");
        let idx = sample();
        idx.write_to_path(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(FuzzyIndex::from_bytes(&bytes).unwrap(), idx);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = sample().to_bytes();
        bytes[0] = b'X';
        // recompute trailer so we exercise the magic check, not the trailer check
        let body_len = bytes.len() - 32;
        let t = blake3::hash(&bytes[..body_len]);
        bytes[body_len..].copy_from_slice(t.as_bytes());
        assert!(matches!(
            FuzzyIndex::from_bytes(&bytes),
            Err(IndexError::BadMagic)
        ));
    }

    #[test]
    fn rejects_unsupported_version() {
        let idx = sample();
        let mut bytes = idx.to_bytes();
        bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
        let body_len = bytes.len() - 32;
        let t = blake3::hash(&bytes[..body_len]);
        bytes[body_len..].copy_from_slice(t.as_bytes());
        assert!(matches!(
            FuzzyIndex::from_bytes(&bytes),
            Err(IndexError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn rejects_unknown_kind() {
        let idx = sample();
        let mut bytes = idx.to_bytes();
        bytes[10..12].copy_from_slice(&9u16.to_le_bytes());
        let body_len = bytes.len() - 32;
        let t = blake3::hash(&bytes[..body_len]);
        bytes[body_len..].copy_from_slice(t.as_bytes());
        assert!(matches!(
            FuzzyIndex::from_bytes(&bytes),
            Err(IndexError::UnknownKind(9))
        ));
    }

    #[test]
    fn rejects_truncated() {
        let bytes = sample().to_bytes();
        let chopped = &bytes[..bytes.len() - 40]; // cut into the body, breaks trailer length too
        assert!(matches!(
            FuzzyIndex::from_bytes(chopped),
            Err(IndexError::Truncated { .. }) | Err(IndexError::TrailerMismatch)
        ));
    }

    #[test]
    fn rejects_trailer_mismatch() {
        let mut bytes = sample().to_bytes();
        let n = bytes.len();
        bytes[n - 1] ^= 0xff; // corrupt the trailer
        assert!(matches!(
            FuzzyIndex::from_bytes(&bytes),
            Err(IndexError::TrailerMismatch)
        ));
    }

    #[test]
    fn from_parts_rejects_wrong_signature_width() {
        let bad = vec![SigEntry {
            row: 0,
            sig: vec![1, 2, 3],
        }]; // 3 != declared 4
        assert!(matches!(
            FuzzyIndex::from_parts(
                SubIndexKind::Minhash,
                [0u8; 32],
                4,
                32,
                4,
                bad,
                BTreeMap::new()
            ),
            Err(IndexError::SignatureWidthMismatch {
                expected: 4,
                got: 3
            })
        ));
    }

    #[test]
    fn body_corruption_caught_by_trailer() {
        let mut bytes = sample().to_bytes();
        // flip a signature byte in the body; trailer must reject it
        bytes[60] ^= 0x01;
        assert!(matches!(
            FuzzyIndex::from_bytes(&bytes),
            Err(IndexError::TrailerMismatch)
        ));
    }
}
