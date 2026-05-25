#!/usr/bin/env python3
"""Generate RFC 6962 + BLAKE3 golden test vectors for attestrum-merkle.

Trillian's published RFC 6962 test vectors are SHA-256-based. attestrum-merkle
substitutes BLAKE3 as the underlying hash function (per BUILD-PLAN.md §3.1
+ §3.2). The leaf and internal-node domain-separation prefix bytes from
RFC 6962 are preserved (0x00 for leaves, 0x01 for internal nodes), but
applied with BLAKE3 instead of SHA-256.

This script re-computes a representative set of test cases under BLAKE3 +
RFC 6962 prefixes and writes the expected roots to `vectors.json`. The
script + vectors.json + this docstring are all checked in so the golden
file is reproducible from source. If a future commit revises the cases,
re-run this script and commit both files together.

Why a Python generator at all (instead of just trusting the Rust impl):
the point of golden vectors is to have an INDEPENDENT computation. The
Rust impl in `crates/attestrum-merkle/src/lib.rs` is what we want to verify;
this Python implementation is the independent oracle. Both implementations
must agree on every case for the test suite to pass.

Usage:
    python3 generate.py > vectors.json

Dependencies:
    blake3 >= 1.0  (PyPI: blake3)

Pinned environment (founder's cross-check venv):
    ~/CascadeProjects/experiments/v02-gate-test/.venv/bin/python
    Confirmed working at blake3 1.0.8 on 2026-05-23.
"""
import json
import sys

import blake3

LEAF_PREFIX = b"\x00"
NODE_PREFIX = b"\x01"


def leaf_hash(leaf: bytes) -> bytes:
    """RFC 6962 leaf hash under BLAKE3: BLAKE3(0x00 || leaf)."""
    return blake3.blake3(LEAF_PREFIX + leaf).digest()


def node_hash(left: bytes, right: bytes) -> bytes:
    """RFC 6962 internal-node hash under BLAKE3: BLAKE3(0x01 || left || right)."""
    return blake3.blake3(NODE_PREFIX + left + right).digest()


def merkle_root(leaves: list) -> bytes:
    """RFC 6962 Merkle root over a list of 32-byte leaves.

    Empty list -> BLAKE3 of empty input.
    Single leaf -> leaf_hash of that leaf.
    Multiple leaves -> iterative pairwise node_hash with RFC 6962
    odd-count carry-up (lone rightmost node carried up unchanged at
    each level).
    """
    if not leaves:
        return blake3.blake3(b"").digest()
    level = [leaf_hash(leaf) for leaf in leaves]
    while len(level) > 1:
        next_level = []
        i = 0
        while i + 1 < len(level):
            next_level.append(node_hash(level[i], level[i + 1]))
            i += 2
        if i < len(level):
            # Lone rightmost node carried up unchanged (RFC 6962
            # odd-count rule). NOT Bitcoin's duplicate-and-hash.
            next_level.append(level[i])
        level = next_level
    return level[0]


def audit_path(leaves: list, m: int) -> list:
    """RFC 6962 audit path (inclusion proof) for leaf at index `m`.

    Walks the tree level-by-level. At each level, records the sibling
    of the node currently containing the leaf's descendant. If the
    descendant is the lone rightmost node at some level (no sibling
    there because of the odd-count carry-up), NO element is appended
    to the path for that level. Path ordering: index 0 is the sibling
    at the deepest level (closest to the leaf); index N-1 is the
    sibling at the shallowest level (closest to the root). This is
    RFC 6962 §2.1.1's convention.
    """
    n = len(leaves)
    assert 0 <= m < n, f"m={m} out of bounds for n={n}"
    if n == 1:
        return []
    path = []
    level = [leaf_hash(leaf) for leaf in leaves]
    idx = m
    while len(level) > 1:
        sibling_idx = idx ^ 1
        if sibling_idx < len(level):
            path.append(level[sibling_idx])
        # Build next level.
        next_level = []
        i = 0
        while i + 1 < len(level):
            next_level.append(node_hash(level[i], level[i + 1]))
            i += 2
        if i < len(level):
            next_level.append(level[i])
        level = next_level
        idx //= 2
    return path


def repeat_byte(b: int) -> str:
    """Return the 64-char hex string of a 32-byte leaf whose every byte is `b`."""
    return f"{b:02x}" * 32


CASES = [
    {
        "name": "empty",
        "leaves_hex": [],
        "comment": "RFC 6962 §2.1: hash of empty list = BLAKE3 of empty input (af1349b9...)",
    },
    {
        "name": "single_zero",
        "leaves_hex": [repeat_byte(0x00)],
        "comment": "single leaf of all-zero bytes; root = leaf_hash(leaves[0])",
    },
    {
        "name": "single_ff",
        "leaves_hex": [repeat_byte(0xFF)],
        "comment": "single leaf of all-0xFF bytes",
    },
    {
        "name": "two_distinct",
        "leaves_hex": [repeat_byte(0x00), repeat_byte(0x01)],
        "comment": "balanced two-leaf tree; root = node_hash(leaf_hash(a), leaf_hash(b))",
    },
    {
        "name": "three_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(3)],
        "comment": "three leaves — exercises RFC 6962 odd-count carry-up at level 1",
    },
    {
        "name": "four_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(4)],
        "comment": "four leaves — fully balanced binary tree (no odd levels)",
    },
    {
        "name": "five_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(5)],
        "comment": "five leaves — RFC 6962 splits at k=4; lone leaf carried to root",
    },
    {
        "name": "six_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(6)],
        "comment": "six leaves — odd at level 2 only",
    },
    {
        "name": "seven_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(7)],
        "comment": "seven leaves — odd at level 1 and level 2",
    },
    {
        "name": "eleven_distinct",
        "leaves_hex": [repeat_byte(i) for i in range(11)],
        "comment": "eleven leaves — non-power-of-2, odd-count carry at multiple levels",
    },
    {
        "name": "multiset_duplicate_pair",
        "leaves_hex": [repeat_byte(0xDE), repeat_byte(0xDE)],
        "comment": "multiset policy: two identical leaves yield a defined root distinct from single-leaf root",
    },
    {
        "name": "multiset_triplicate",
        "leaves_hex": [repeat_byte(0xDE)] * 3,
        "comment": "multiset: three identical leaves — odd-count rule applies as for distinct leaves",
    },
    {
        "name": "multiset_pair_with_distinct",
        "leaves_hex": [repeat_byte(0xDE), repeat_byte(0xDE), repeat_byte(0xEF)],
        "comment": "multiset: two identical leaves followed by a distinct one",
    },
]


def main() -> int:
    out = {
        "description": "RFC 6962 + BLAKE3 golden vectors for attestrum-merkle (Sprint 2 E7 + E8).",
        "leaf_prefix": "0x00",
        "node_prefix": "0x01",
        "hash_function": "BLAKE3",
        "generator": "crates/attestrum-merkle/tests/fixtures/rfc6962/generate.py",
        "cases": [],
    }
    for case in CASES:
        leaves = [bytes.fromhex(h) for h in case["leaves_hex"]]
        # Validate every leaf is 32 bytes (attestrum-merkle::merkle_root takes
        # &[[u8; 32]] only — variable-length leaves not supported by v1).
        for leaf in leaves:
            assert len(leaf) == 32, f"case {case['name']!r}: leaf must be 32 bytes, got {len(leaf)}"
        root = merkle_root(leaves).hex()
        case_out = {
            "name": case["name"],
            "leaves_hex": case["leaves_hex"],
            "expected_root_hex": root,
            "comment": case["comment"],
        }
        # Audit paths for every leaf index, when the tree is non-empty.
        # Locks the RFC 6962 §2.1.1 index convention in golden form so
        # any future change to audit_path's algorithm fails this test.
        if leaves:
            case_out["audit_paths"] = [
                {
                    "leaf_index": m,
                    "expected_path_hex": [h.hex() for h in audit_path(leaves, m)],
                }
                for m in range(len(leaves))
            ]
        out["cases"].append(case_out)
    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
