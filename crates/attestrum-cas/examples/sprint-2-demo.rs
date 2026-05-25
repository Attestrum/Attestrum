//! Sprint 2 acceptance demo.
//!
//! End-to-end walkthrough of everything Sprint 2 shipped: streaming
//! BLAKE3+SHA-256 hashing (E5), atomic CAS put + roundtrip (E6),
//! RFC 6962 Merkle root over sorted digests (E7), and audit-path
//! generation + verification (E8). Tiny in-memory corpus, no I/O
//! besides the CAS writes into a per-process tempdir.
//!
//! Output is captured into `docs/demos/sprint-2.cast` via the
//! checked-in generator at `tools/cast/sprint-2.py` (Python-generated
//! for JSON-escape safety, same pattern as Sprint 1 E12). Re-run the
//! generator after changing the demo body so the cast stays in sync.

use std::fs;
use std::io::Read;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use attestrum_cas::{stream_hash, CasStore};
use attestrum_core::hex;
use attestrum_merkle::{audit_path, merkle_root, verify_audit_path, MerkleTree};

const CORPUS: &[&str] = &[
    "Attestrum Sprint 2 demo — document zero.",
    "Document one. Slightly longer than zero, with a different ending.",
    "Doc two. The third leaf in the corpus.",
    "Fourth document; about-average length and contents.",
    "Fifth and final document of this acceptance demo.",
];

fn short_hex(bytes: &[u8; 32]) -> String {
    let full = hex::encode_32(bytes);
    format!("{}..{}", &full[..12], &full[full.len() - 4..])
}

fn main() -> std::io::Result<()> {
    println!("=== Attestrum Sprint 2 acceptance demo ===");
    println!();

    // ---------------------------------------------------------------
    // Corpus
    // ---------------------------------------------------------------
    println!("--- 5-document in-memory corpus ---");
    for (i, doc) in CORPUS.iter().enumerate() {
        println!("  [{i}] {:>3} bytes  {doc:?}", doc.len());
    }
    println!();

    // ---------------------------------------------------------------
    // E5: streaming BLAKE3 + SHA-256
    // ---------------------------------------------------------------
    println!("--- E5: streaming BLAKE3 + SHA-256 hasher ---");
    let hashes: Vec<_> = CORPUS
        .iter()
        .map(|doc| stream_hash(doc.as_bytes()).expect("hash"))
        .collect();
    for (i, h) in hashes.iter().enumerate() {
        println!(
            "  [{i}] blake3={}  sha256={}  size={}B",
            short_hex(&h.blake3),
            short_hex(&h.sha256),
            h.size_bytes
        );
    }
    println!();

    // ---------------------------------------------------------------
    // E6: CasStore atomic put + roundtrip
    // ---------------------------------------------------------------
    println!("--- E6: CasStore atomic put + roundtrip ---");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let cas_root = std::env::temp_dir().join(format!(
        "attestrum-sprint-2-demo-{}-{}",
        process::id(),
        nanos
    ));
    let store = CasStore::new(&cas_root)?;
    println!("  CAS root: {}", cas_root.display());
    for (i, (doc, h)) in CORPUS.iter().zip(&hashes).enumerate() {
        store.put(&h.blake3, doc.as_bytes())?;
        let rel = store
            .path_for(&h.blake3)
            .strip_prefix(&cas_root)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| store.path_for(&h.blake3));
        println!("  [{i}] put -> {}", rel.display());
    }
    let mut roundtrip_ok = 0;
    for (doc, h) in CORPUS.iter().zip(&hashes) {
        let mut file = store.open(&h.blake3)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        if buf.as_slice() == doc.as_bytes() {
            roundtrip_ok += 1;
        }
    }
    println!(
        "  roundtrip via CasStore::open: {roundtrip_ok}/{} match",
        CORPUS.len()
    );
    println!();

    // ---------------------------------------------------------------
    // E7: RFC 6962 Merkle root over sorted BLAKE3 digests
    // ---------------------------------------------------------------
    println!("--- E7: RFC 6962 Merkle root (BLAKE3, sorted, multiset) ---");
    let mut digests: Vec<[u8; 32]> = hashes.iter().map(|h| h.blake3).collect();
    digests.sort_unstable();
    println!("  sorted digests:");
    for d in &digests {
        println!("    {}", short_hex(d));
    }
    let tree = MerkleTree::new(digests.clone());
    let root = tree.root();
    println!("  root: {}", hex::encode_32(&root));
    println!("        (== merkle_root over the sorted leaves)");
    assert_eq!(root, merkle_root(&digests));
    println!();

    // ---------------------------------------------------------------
    // E8: audit path generate + verify on one leaf
    // ---------------------------------------------------------------
    println!("--- E8: audit path generate + verify ---");
    let target_leaf = 2usize;
    let target_digest = digests[target_leaf];
    let path = audit_path(&tree, target_leaf).expect("in-bounds");
    println!(
        "  leaf {target_leaf} (digest {}):",
        short_hex(&target_digest)
    );
    println!(
        "  audit path (length {}, path[0] = sibling closest to leaf):",
        path.len()
    );
    for (i, sib) in path.iter().enumerate() {
        println!("    [{i}] {}", short_hex(sib));
    }
    let ok = verify_audit_path(&root, &target_digest, target_leaf, digests.len(), &path);
    println!(
        "  verify_audit_path(root, leaf, {target_leaf}, {}, path) -> {ok}",
        digests.len()
    );
    assert!(
        ok,
        "verify_audit_path must succeed for a freshly-generated path"
    );
    println!();

    // ---------------------------------------------------------------
    // Cleanup + footer
    // ---------------------------------------------------------------
    let _ = fs::remove_dir_all(&cas_root);
    println!("=== SPRINT 2 COMPLETE ===");
    println!("    streaming hash + atomic CAS + Merkle root + audit path = green");
    Ok(())
}
