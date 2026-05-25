//! `attestrum-merkle` — RFC 6962 binary Merkle tree over BLAKE3.
//!
//! PROTECTED per CLAUDE.md §4 once shipped. The choice of hash
//! function, the leaf and internal domain-separation prefix bytes,
//! and the RFC 6962 odd-count carry-up rule are all corpus-incompatible
//! contracts: changing any of them invalidates every signed bundle
//! Attestrum has ever issued.
//!
//! # Algorithm
//!
//! Following RFC 6962 §2.1 with BLAKE3 substituted for SHA-256:
//!
//! ```text
//! leaf_hash(leaf)        = BLAKE3(0x00 || leaf)
//! node_hash(left, right) = BLAKE3(0x01 || left || right)
//! ```
//!
//! The Merkle Tree Hash (`MTH`) is defined recursively:
//!
//! ```text
//! MTH({})    = BLAKE3("")                        // empty input
//! MTH({d})   = leaf_hash(d)                       // single leaf
//! MTH(D_n)   = node_hash(MTH(D[0:k]), MTH(D[k:n]))
//!              where k is the largest power of 2 strictly less than n
//! ```
//!
//! Implemented iteratively level-by-level for cache friendliness; the
//! iterative form is equivalent to the recursive definition for all
//! input sizes, including non-power-of-2 cases. At each level, pairs of
//! adjacent nodes are combined via `node_hash`; if the level has an
//! odd number of nodes, the lone rightmost node is **carried up
//! unchanged** to the next level. This is the RFC 6962 odd-count rule
//! — NOT Bitcoin's "duplicate the lone node" convention.
//!
//! # Duplicate-leaf policy: multiset
//!
//! Identical leaf bytes produce identical leaf hashes and are passed
//! as two adjacent entries in `leaves`, not deduplicated. The Merkle
//! root attests to the corpus *as compiled* (a multiset), not to
//! "these unique blobs exist." See `docs/diagrams/sprint-2/merkle-construction.md`
//! and the founder-approved Sprint 2 amendment 2 (2026-05-23).
//!
//! # Leaf order
//!
//! `merkle_root` does NOT sort leaves internally. Callers are expected
//! to sort leaves by BLAKE3 digest before invoking `merkle_root` so
//! the root is invariant under input permutation. Attestrum's pipeline
//! (Sprint 3+) sorts before calling.
//!
//! # Sprint 2 surface
//!
//! E7 ships construction (`leaf_hash`, `node_hash`, `merkle_root`,
//! `MerkleTree`). E8 extends with audit-path generation
//! ([`audit_path`], [`MerkleTree::audit_path`]) and verification
//! ([`verify_audit_path`]) per RFC 6962 §2.1.1.
//!
//! ## Audit-path index convention (PROTECTED)
//!
//! Path element ordering: `path[0]` is the sibling at the deepest
//! level (closest to the leaf); `path[path.len() - 1]` is the
//! sibling at the shallowest level (closest to the root). This
//! matches RFC 6962 §2.1.1's left-to-right concatenation in the
//! recursive `PATH(m, D[n])` definition. Any future change to this
//! ordering would invalidate every previously-emitted inclusion
//! proof and is a PROTECTED-system change.
//!
//! Odd-count carry-up implication: when a leaf's descendant is the
//! lone rightmost node at some level (no sibling at that level),
//! NO path element is consumed for that level. Path length therefore
//! depends on both `tree_size` and `leaf_index`, not just `tree_size`.

/// RFC 6962 leaf-hash domain separation byte. Prepended to every leaf
/// before BLAKE3.
const LEAF_PREFIX: u8 = 0x00;

/// RFC 6962 internal-node-hash domain separation byte. Prepended to the
/// `left || right` concatenation of two child hashes before BLAKE3.
const NODE_PREFIX: u8 = 0x01;

/// Compute the RFC 6962 leaf hash of `leaf`: `BLAKE3(0x00 || leaf)`.
///
/// The single-byte prefix prevents second-preimage attacks where an
/// attacker substitutes an internal-node hash sequence as a fake leaf
/// (or vice versa).
pub fn leaf_hash(leaf: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[LEAF_PREFIX]);
    hasher.update(leaf);
    *hasher.finalize().as_bytes()
}

/// Compute the RFC 6962 internal-node hash:
/// `BLAKE3(0x01 || left || right)`.
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[NODE_PREFIX]);
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

/// Compute the RFC 6962 Merkle root over `leaves`.
///
/// Each entry in `leaves` is treated as an RFC 6962 leaf and first
/// passed through [`leaf_hash`]. For Attestrum's pipeline these are the
/// BLAKE3 digests of corpus documents; the Merkle tree commits to the
/// (sorted) multiset of digests.
///
/// - Empty `leaves` → `BLAKE3("")` (RFC 6962 §2.1: hash of empty list
///   is hash of empty input).
/// - Single leaf → [`leaf_hash`] of that leaf.
/// - Multiple leaves → iterative pairwise combination via [`node_hash`]
///   with the RFC 6962 odd-count carry-up rule.
///
/// See the multiset duplicate-leaf policy in this crate's top-level
/// docs.
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return *blake3::Hasher::new().finalize().as_bytes();
    }
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|leaf| leaf_hash(leaf)).collect();
    while level.len() > 1 {
        let mut next: Vec<[u8; 32]> = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < level.len() {
            next.push(node_hash(&level[i], &level[i + 1]));
            i += 2;
        }
        if i < level.len() {
            // Lone rightmost node carried up unchanged (RFC 6962
            // odd-count rule). NOT Bitcoin's duplicate-and-hash.
            next.push(level[i]);
        }
        level = next;
    }
    level[0]
}

/// A Merkle tree owning its sorted leaf set, ready to compute roots
/// and (Sprint 2 E8) audit paths.
///
/// `MerkleTree` is intentionally minimal in E7 — just a typed
/// wrapper around the leaf set with a `root()` convenience. E8 adds
/// `audit_path` and `verify_audit_path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleTree {
    leaves: Vec<[u8; 32]>,
}

impl MerkleTree {
    /// Build a tree from `leaves`. The caller is responsible for
    /// sort order (Attestrum sorts by BLAKE3 digest before construction).
    /// Duplicate leaves are preserved (multiset policy).
    pub fn new(leaves: Vec<[u8; 32]>) -> Self {
        Self { leaves }
    }

    /// Borrow the underlying leaf slice.
    pub fn leaves(&self) -> &[[u8; 32]] {
        &self.leaves
    }

    /// Number of leaves in the tree (may include duplicates).
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// `true` if the tree has zero leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Compute the RFC 6962 Merkle root over the owned leaf set.
    pub fn root(&self) -> [u8; 32] {
        merkle_root(&self.leaves)
    }

    /// Generate the RFC 6962 audit path (inclusion proof) for the
    /// leaf at `leaf_index`. Convenience wrapper around the free
    /// [`audit_path`] function.
    pub fn audit_path(&self, leaf_index: usize) -> Result<Vec<[u8; 32]>, AuditPathError> {
        audit_path(self, leaf_index)
    }
}

/// Errors returned by [`audit_path`] / [`MerkleTree::audit_path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditPathError {
    /// `leaf_index >= tree.len()`. Audit paths can only be generated
    /// for in-bounds leaves.
    IndexOutOfBounds { leaf_index: usize, tree_size: usize },
}

impl core::fmt::Display for AuditPathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::IndexOutOfBounds {
                leaf_index,
                tree_size,
            } => write!(
                f,
                "audit_path: leaf_index {leaf_index} out of bounds for tree of size {tree_size}"
            ),
        }
    }
}

impl core::error::Error for AuditPathError {}

/// Generate the RFC 6962 audit path (inclusion proof) for `leaf_index`
/// in `tree`.
///
/// Walks the tree level-by-level (mirroring [`merkle_root`]). At each
/// level, records the sibling of the node currently holding the leaf's
/// descendant. If the descendant is the lone rightmost node at some
/// level (no sibling there because of the odd-count carry-up), NO
/// element is appended to the path for that level.
///
/// Path ordering matches RFC 6962 §2.1.1: `path[0]` is the sibling at
/// the deepest level (closest to the leaf); the final element is the
/// sibling at the shallowest level (closest to the root). See this
/// crate's top-level docs for the full PROTECTED-system index
/// convention.
///
/// Edge cases:
/// - Empty tree → [`AuditPathError::IndexOutOfBounds`].
/// - `leaf_index >= tree.len()` → [`AuditPathError::IndexOutOfBounds`].
/// - Single-leaf tree → `Ok(vec![])` (the leaf hash IS the root; no
///   path needed).
pub fn audit_path(tree: &MerkleTree, leaf_index: usize) -> Result<Vec<[u8; 32]>, AuditPathError> {
    let tree_size = tree.leaves.len();
    if leaf_index >= tree_size {
        return Err(AuditPathError::IndexOutOfBounds {
            leaf_index,
            tree_size,
        });
    }
    if tree_size == 1 {
        return Ok(Vec::new());
    }
    let mut path: Vec<[u8; 32]> = Vec::new();
    let mut level: Vec<[u8; 32]> = tree.leaves.iter().map(|leaf| leaf_hash(leaf)).collect();
    let mut idx = leaf_index;
    while level.len() > 1 {
        let sibling_idx = idx ^ 1;
        if sibling_idx < level.len() {
            path.push(level[sibling_idx]);
        }
        // Build next level using the same odd-count carry-up rule as
        // merkle_root, so the level traversal in audit_path stays in
        // sync with what merkle_root would produce.
        let mut next: Vec<[u8; 32]> = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < level.len() {
            next.push(node_hash(&level[i], &level[i + 1]));
            i += 2;
        }
        if i < level.len() {
            next.push(level[i]);
        }
        level = next;
        idx /= 2;
    }
    Ok(path)
}

/// Verify an RFC 6962 inclusion proof. Returns `true` iff the
/// recomputed root equals the supplied `root` for the given
/// `(leaf, leaf_index, tree_size, path)` tuple.
///
/// Replays the level-by-level walk used by [`audit_path`]: starts
/// with `leaf_hash(leaf)` as the running hash, then at each level
/// either combines with a path-supplied sibling (when one exists at
/// this level) or carries the running hash up unchanged (when the
/// descendant is the lone rightmost node). Returns `false` for ANY
/// inconsistency: out-of-bounds index, path length wrong for the
/// (leaf_index, tree_size) combination, root mismatch, or any
/// modified element along the way.
///
/// `tree_size == 1` is special-cased: the proof must be empty and
/// `root` must equal `leaf_hash(leaf)`.
///
/// `tree_size == 0` is rejected (no valid root exists for an empty
/// tree's inclusion proof — by definition there are no leaves to
/// include).
pub fn verify_audit_path(
    root: &[u8; 32],
    leaf: &[u8; 32],
    leaf_index: usize,
    tree_size: usize,
    path: &[[u8; 32]],
) -> bool {
    if tree_size == 0 || leaf_index >= tree_size {
        return false;
    }
    if tree_size == 1 {
        return path.is_empty() && root == &leaf_hash(leaf);
    }
    let mut current = leaf_hash(leaf);
    let mut path_iter = path.iter();
    let mut idx = leaf_index;
    let mut level_size = tree_size;
    while level_size > 1 {
        let sibling_idx = idx ^ 1;
        if sibling_idx < level_size {
            // We have a sibling at this level; consume one path element.
            let sibling = match path_iter.next() {
                Some(s) => s,
                None => return false, // path too short
            };
            current = if idx % 2 == 0 {
                node_hash(&current, sibling)
            } else {
                node_hash(sibling, &current)
            };
        }
        // else: lone rightmost at this level — current carries up
        // unchanged with no path element consumed.
        idx /= 2;
        level_size = level_size.div_ceil(2);
    }
    // Path must be fully consumed; any leftover means the proof was
    // too long for the (leaf_index, tree_size) combination and is
    // invalid.
    if path_iter.next().is_some() {
        return false;
    }
    &current == root
}

#[cfg(test)]
mod tests {
    use super::*;

    // BLAKE3 of empty input. Canonical reference vector from the BLAKE3
    // specification.
    const BLAKE3_EMPTY: [u8; 32] = [
        0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9,
        0x49, 0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f,
        0x32, 0x62,
    ];

    #[test]
    fn empty_tree_returns_blake3_of_empty() {
        assert_eq!(merkle_root(&[]), BLAKE3_EMPTY);
    }

    #[test]
    fn leaf_hash_prefixes_with_zero_byte() {
        // leaf_hash(b"") = BLAKE3(0x00 || "") = BLAKE3(&[0x00]).
        let direct = *blake3::Hasher::new().update(&[0x00]).finalize().as_bytes();
        assert_eq!(leaf_hash(b""), direct);
    }

    #[test]
    fn node_hash_prefixes_with_one_byte() {
        let left = [0x11u8; 32];
        let right = [0x22u8; 32];
        let direct = *blake3::Hasher::new()
            .update(&[0x01])
            .update(&left)
            .update(&right)
            .finalize()
            .as_bytes();
        assert_eq!(node_hash(&left, &right), direct);
    }

    #[test]
    fn single_leaf_tree_equals_leaf_hash() {
        let leaf = [0xa5u8; 32];
        assert_eq!(merkle_root(&[leaf]), leaf_hash(&leaf));
    }

    #[test]
    fn two_leaf_tree_equals_node_hash_of_leaf_hashes() {
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        let expected = node_hash(&leaf_hash(&a), &leaf_hash(&b));
        assert_eq!(merkle_root(&[a, b]), expected);
    }

    #[test]
    fn three_leaf_tree_uses_rfc6962_odd_count_carry_up() {
        // Per RFC 6962, n=3 → k=2 (largest power of 2 < 3). Tree shape:
        //         root
        //        /    \
        //   N(LH0,LH1) LH2     ← LH(c) carried up unchanged at level 1
        //    / \
        //  LH0 LH1
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        let c = [0x03u8; 32];
        let expected = node_hash(&node_hash(&leaf_hash(&a), &leaf_hash(&b)), &leaf_hash(&c));
        assert_eq!(merkle_root(&[a, b, c]), expected);
    }

    #[test]
    fn three_leaf_tree_is_not_bitcoin_duplication() {
        // Bitcoin's Merkle would duplicate the lone leaf:
        //   bitcoin_root = NH(NH(LH(a), LH(b)), NH(LH(c), LH(c)))
        // RFC 6962 does NOT do this. Assert they differ.
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        let c = [0x03u8; 32];
        let lh_c = leaf_hash(&c);
        let bitcoin_root = node_hash(
            &node_hash(&leaf_hash(&a), &leaf_hash(&b)),
            &node_hash(&lh_c, &lh_c),
        );
        assert_ne!(merkle_root(&[a, b, c]), bitcoin_root);
    }

    #[test]
    fn multiset_duplicate_leaves_yield_distinct_root_from_single() {
        // Founder-approved 2026-05-23 (Sprint 2 amendment 2): identical
        // leaves are NOT deduplicated. Two copies of the same leaf
        // produce node_hash(leaf_hash(x), leaf_hash(x)), which differs
        // from leaf_hash(x) alone.
        let x = [0xdeu8; 32];
        let single = merkle_root(&[x]);
        let pair = merkle_root(&[x, x]);
        assert_ne!(single, pair);
        assert_eq!(pair, node_hash(&leaf_hash(&x), &leaf_hash(&x)));
    }

    #[test]
    fn multiset_triplicate_leaves_have_their_own_root() {
        // Three copies of the same leaf: shape is N(N(LH, LH), LH)
        // because the third copy is the lone right node carried up.
        let x = [0xdeu8; 32];
        let lh = leaf_hash(&x);
        let expected = node_hash(&node_hash(&lh, &lh), &lh);
        assert_eq!(merkle_root(&[x, x, x]), expected);
    }

    #[test]
    fn root_is_deterministic_across_calls() {
        let mut leaves = vec![[0u8; 32]; 100];
        for (i, leaf) in leaves.iter_mut().enumerate() {
            leaf[0] = i as u8;
            leaf[31] = (i ^ 0x5a) as u8;
        }
        let first = merkle_root(&leaves);
        let second = merkle_root(&leaves);
        assert_eq!(first, second);
    }

    #[test]
    fn root_depends_on_order_when_caller_does_not_sort() {
        let a = [0x01u8; 32];
        let b = [0x02u8; 32];
        // [a, b] and [b, a] produce DIFFERENT roots because merkle_root
        // does not sort internally. The caller must sort by digest first
        // for permutation-invariance.
        assert_ne!(merkle_root(&[a, b]), merkle_root(&[b, a]));
    }

    #[test]
    fn merkletree_wrapper_matches_free_function() {
        let leaves: Vec<[u8; 32]> = (0..7u8).map(|i| [i; 32]).collect();
        let tree = MerkleTree::new(leaves.clone());
        assert_eq!(tree.root(), merkle_root(&leaves));
        assert_eq!(tree.len(), 7);
        assert!(!tree.is_empty());
        assert_eq!(tree.leaves(), leaves.as_slice());
    }

    #[test]
    fn empty_merkletree_root_matches_free_function() {
        let tree = MerkleTree::new(vec![]);
        assert!(tree.is_empty());
        assert_eq!(tree.root(), merkle_root(&[]));
        assert_eq!(tree.root(), BLAKE3_EMPTY);
    }

    // ---------------------------------------------------------------
    // Sprint 2 E8: audit_path / verify_audit_path
    // ---------------------------------------------------------------

    fn distinct_leaves(n: usize) -> Vec<[u8; 32]> {
        (0..n).map(|i| [i as u8; 32]).collect()
    }

    #[test]
    fn audit_path_empty_tree_is_out_of_bounds() {
        let tree = MerkleTree::new(vec![]);
        let err = tree.audit_path(0).expect_err("expected error");
        assert_eq!(
            err,
            AuditPathError::IndexOutOfBounds {
                leaf_index: 0,
                tree_size: 0
            }
        );
    }

    #[test]
    fn audit_path_out_of_bounds_index() {
        let tree = MerkleTree::new(distinct_leaves(5));
        let err = tree.audit_path(5).expect_err("expected error");
        assert_eq!(
            err,
            AuditPathError::IndexOutOfBounds {
                leaf_index: 5,
                tree_size: 5
            }
        );
        // Way-out-of-bounds also rejected.
        assert!(tree.audit_path(999).is_err());
    }

    #[test]
    fn audit_path_single_leaf_is_empty() {
        let tree = MerkleTree::new(distinct_leaves(1));
        let path = tree.audit_path(0).expect("single-leaf path");
        assert!(path.is_empty());
    }

    #[test]
    fn audit_path_single_leaf_roundtrip_verifies() {
        let leaves = distinct_leaves(1);
        let tree = MerkleTree::new(leaves.clone());
        let path = tree.audit_path(0).expect("single-leaf path");
        assert!(verify_audit_path(&tree.root(), &leaves[0], 0, 1, &path));
    }

    #[test]
    fn audit_path_three_leaf_known_paths() {
        // From RFC 6962 §2.1.1 recursive definition, hand-traced:
        //   m=0: path = [LH(d1), LH(d2)]
        //   m=1: path = [LH(d0), LH(d2)]
        //   m=2: path = [NH(LH(d0), LH(d1))]
        let leaves = distinct_leaves(3);
        let tree = MerkleTree::new(leaves.clone());
        let lh0 = leaf_hash(&leaves[0]);
        let lh1 = leaf_hash(&leaves[1]);
        let lh2 = leaf_hash(&leaves[2]);
        assert_eq!(tree.audit_path(0).unwrap(), vec![lh1, lh2]);
        assert_eq!(tree.audit_path(1).unwrap(), vec![lh0, lh2]);
        assert_eq!(tree.audit_path(2).unwrap(), vec![node_hash(&lh0, &lh1)]);
    }

    #[test]
    fn audit_path_roundtrip_verifies_every_leaf_for_sizes_2_through_15() {
        // For each tree size 2..=15, verify every leaf's audit path
        // roundtrips correctly. Exercises balanced + unbalanced sizes
        // and every odd-count carry configuration up to depth 4.
        for n in 2..=15 {
            let leaves = distinct_leaves(n);
            let tree = MerkleTree::new(leaves.clone());
            let root = tree.root();
            for (m, leaf) in leaves.iter().enumerate() {
                let path = tree.audit_path(m).expect("in-bounds path");
                assert!(
                    verify_audit_path(&root, leaf, m, n, &path),
                    "roundtrip failed for n={n}, m={m}, path_len={}",
                    path.len()
                );
            }
        }
    }

    #[test]
    fn audit_path_length_matches_levels_with_a_sibling() {
        // For n=5, m=4, the lone-rightmost leaf carries up through
        // levels 0 and 1 (no sibling), gaining a sibling only at the
        // root level. Path length = 1.
        let tree = MerkleTree::new(distinct_leaves(5));
        assert_eq!(tree.audit_path(4).unwrap().len(), 1);
        // For n=5, m=0, sibling at every level (level 0: LH(d1); level
        // 1: NH(LH(d2), LH(d3)); level 2: LH(d4)).
        assert_eq!(tree.audit_path(0).unwrap().len(), 3);
        // For n=4 (balanced), every leaf has the same path length = 2.
        let balanced = MerkleTree::new(distinct_leaves(4));
        for m in 0..4 {
            assert_eq!(balanced.audit_path(m).unwrap().len(), 2);
        }
    }

    #[test]
    fn verify_rejects_modified_leaf() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let path = tree.audit_path(3).expect("path");
        let mut tampered = leaves[3];
        tampered[0] ^= 0xff;
        assert!(!verify_audit_path(&root, &tampered, 3, 7, &path));
    }

    #[test]
    fn verify_rejects_wrong_leaf_index() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let path = tree.audit_path(3).expect("path");
        // Same leaf bytes, same path, but claim a different index.
        assert!(!verify_audit_path(&root, &leaves[3], 4, 7, &path));
        assert!(!verify_audit_path(&root, &leaves[3], 0, 7, &path));
    }

    #[test]
    fn verify_rejects_modified_path_element() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let path = tree.audit_path(3).expect("path");
        for i in 0..path.len() {
            let mut tampered = path.clone();
            tampered[i][0] ^= 0x01;
            assert!(
                !verify_audit_path(&root, &leaves[3], 3, 7, &tampered),
                "tampering with path element {i} should have invalidated the proof"
            );
        }
    }

    #[test]
    fn verify_rejects_too_short_path() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let path = tree.audit_path(0).expect("path"); // length 3
        let truncated = &path[..path.len() - 1];
        assert!(!verify_audit_path(&root, &leaves[0], 0, 7, truncated));
    }

    #[test]
    fn verify_rejects_too_long_path() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let mut path = tree.audit_path(0).expect("path");
        path.push([0u8; 32]); // extraneous trailing element
        assert!(!verify_audit_path(&root, &leaves[0], 0, 7, &path));
    }

    #[test]
    fn verify_rejects_out_of_bounds_index() {
        let leaves = distinct_leaves(7);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        let path = tree.audit_path(0).expect("path");
        assert!(!verify_audit_path(&root, &leaves[0], 7, 7, &path));
        assert!(!verify_audit_path(&root, &leaves[0], 999, 7, &path));
    }

    #[test]
    fn verify_rejects_tree_size_zero() {
        // Empty tree's "root" is BLAKE3 of empty input, but no
        // inclusion proof is meaningful for it.
        let leaf = [0xaau8; 32];
        assert!(!verify_audit_path(&BLAKE3_EMPTY, &leaf, 0, 0, &[]));
    }

    #[test]
    fn verify_single_leaf_requires_empty_path() {
        let leaves = distinct_leaves(1);
        let tree = MerkleTree::new(leaves.clone());
        let root = tree.root();
        assert!(verify_audit_path(&root, &leaves[0], 0, 1, &[]));
        // Non-empty path on a single-leaf tree is invalid.
        assert!(!verify_audit_path(&root, &leaves[0], 0, 1, &[[0u8; 32]]));
        // Wrong leaf on a single-leaf tree is invalid.
        let wrong = [0xffu8; 32];
        assert!(!verify_audit_path(&root, &wrong, 0, 1, &[]));
    }

    #[test]
    fn verify_rejects_wrong_root() {
        let leaves = distinct_leaves(5);
        let tree = MerkleTree::new(leaves.clone());
        let path = tree.audit_path(2).expect("path");
        let wrong_root = [0u8; 32];
        assert!(!verify_audit_path(&wrong_root, &leaves[2], 2, 5, &path));
    }
}
