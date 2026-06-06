//! v1.1 fuzzy-index fast-path integration tests. The load-bearing property:
//! the indexed `dispatch_*` path returns a proof byte-identical to the
//! exhaustive scan (`--no-index`) — same matched leaf, same evidence, same
//! confidence. Plus the safety fallbacks: a stale or corrupt sidecar silently
//! reverts to the exhaustive scan and still proves correctly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::CasStore;
use attestrum_core::Modality;
use attestrum_index::build::build_all;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, InclusionProofPredicate, ManifestSource, ProofKind, ProofTarget, ProveOpts,
};

const SDE: i64 = 1_700_000_000;
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-fuzzy-index-{name}-{n}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
    std::fs::create_dir_all(&root).expect("create root");
    root
}

fn opts(root: &Path, no_index: bool) -> ProveOpts {
    ProveOpts {
        sign: false,
        source_date_epoch: SDE,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: Some(root.join("cas-root")),
        no_index,
    }
}

fn seal(root: &Path, items: &[(&[u8], Modality)]) -> PathBuf {
    let cas = CasStore::new(root.join("cas-root")).expect("cas");
    let mut entries: Vec<ManifestEntry> = items
        .iter()
        .map(|(bytes, modality)| {
            let h = attestrum_cas::stream_hash(*bytes).expect("hash");
            cas.put(&h.blake3, bytes).expect("put");
            ManifestEntry {
                document_id: h.blake3,
                sha256: h.sha256,
                size_bytes: h.size_bytes,
                modality: *modality,
                mime_type: None,
                source_url: None,
                source_type: None,
                source_dataset_id: None,
                registered_domain: None,
                license_spdx: None,
                language: None,
                fetched_at: None,
                signals: ManifestSignals::default(),
                included: true,
                exclusion_reason: None,
                chunk_refs: None,
                input_ordinal: 0,
                occurrence_index: 0,
            }
        })
        .collect();
    assign_input_ordinals(&mut entries);
    sort_entries(&mut entries);
    assign_occurrence_indices(&mut entries);
    let manifest = root.join("manifest.parquet");
    write_manifest(&manifest, &entries).expect("write manifest");
    manifest
}

// A leaf long enough that a case/whitespace-only variant stays a near-duplicate
// (misses the exact path, hits the fuzzy modes).
const LEAF: &[u8] = b"The quick brown fox jumps over the lazy dog while the \
    industrious bee gathers nectar from the blooming wildflowers under a warm \
    afternoon sun and a gentle breeze drifts across the quiet meadow";
const OTHER: &[u8] =
    b"financial regulators published new capital adequacy guidance for institutions";
// Same normalized content as LEAF (lowercased, whitespace collapsed) but
// different raw bytes → misses exact, resolves via a fuzzy mode.
const PROBE: &[u8] = b"the QUICK brown fox jumps over the lazy dog while the \
    industrious bee gathers   nectar from the blooming wildflowers under a warm \
    afternoon sun and a gentle breeze drifts across the quiet meadow";

fn inclusion_evidence(root: &Path, manifest: &Path, no_index: bool) -> (f32, String) {
    let doc = root.join("probe.txt");
    std::fs::write(&doc, PROBE).expect("write probe");
    let artifact = prove(
        ProofTarget::Document(doc),
        ManifestSource::Local(manifest.to_path_buf()),
        &opts(root, no_index),
    )
    .expect("probe proves");
    assert_eq!(artifact.kind, ProofKind::Inclusion);
    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse predicate");
    (
        artifact.confidence,
        serde_json::to_string(&pred.match_evidence).expect("evidence json"),
    )
}

#[test]
fn indexed_matches_exhaustive() {
    let root = fresh_root("equiv");
    let manifest = seal(&root, &[(LEAF, Modality::Text), (OTHER, Modality::Text)]);
    build_all(&manifest, &root.join("cas-root"), SDE).expect("build index");

    let exhaustive = inclusion_evidence(&root, &manifest, true);
    let indexed = inclusion_evidence(&root, &manifest, false);
    assert_eq!(
        indexed, exhaustive,
        "indexed fast-path must produce the identical (confidence, evidence) as the exhaustive scan"
    );
}

#[test]
fn corrupt_sidecar_falls_back_to_exhaustive() {
    let root = fresh_root("corrupt");
    let manifest = seal(&root, &[(LEAF, Modality::Text), (OTHER, Modality::Text)]);
    build_all(&manifest, &root.join("cas-root"), SDE).expect("build index");

    let want = inclusion_evidence(&root, &manifest, true); // exhaustive reference

    // Corrupt every minhash sidecar byte → from_bytes rejects → fallback.
    let sidecar = root
        .join("cas-root")
        .join("index")
        .join("minhash")
        .join("v1.idx");
    let mut bytes = std::fs::read(&sidecar).expect("read sidecar");
    for b in bytes.iter_mut() {
        *b ^= 0xff;
    }
    std::fs::write(&sidecar, &bytes).expect("corrupt sidecar");

    let got = inclusion_evidence(&root, &manifest, false); // auto-detect → corrupt → fallback
    assert_eq!(
        got, want,
        "corrupt sidecar must fall back and still prove correctly"
    );
}

#[test]
fn stale_binding_falls_back_to_exhaustive() {
    let root = fresh_root("stale");
    // Index binds to the 2-leaf corpus...
    let manifest1 = seal(&root, &[(LEAF, Modality::Text), (OTHER, Modality::Text)]);
    build_all(&manifest1, &root.join("cas-root"), SDE).expect("build index");
    // ...then the manifest gains a third leaf (different Merkle root). The old
    // sidecar's BINDING_ROOT no longer matches → the querier must reject it.
    let third = b"a third document entirely about astronomy and distant galaxies";
    let manifest2 = seal(
        &root,
        &[
            (LEAF, Modality::Text),
            (OTHER, Modality::Text),
            (third, Modality::Text),
        ],
    );

    let want = inclusion_evidence(&root, &manifest2, true); // exhaustive over the 3-leaf corpus
    let got = inclusion_evidence(&root, &manifest2, false); // stale index → fallback
    assert_eq!(
        got, want,
        "stale-binding index must fall back and still prove correctly"
    );
}

/// Honest wall-clock benchmark: indexed vs `--no-index` over a several-hundred
/// leaf corpus. `#[ignore]` keeps it out of the default suite (timing is
/// environment-dependent); run on demand with
/// `cargo test -p attestrum-prove --test fuzzy_index -- --ignored --nocapture`.
/// It asserts (a) the two paths return the identical proof and (b) the indexed
/// query is faster — the exhaustive path re-fingerprints every leaf for both the
/// ISCC and MinHash scans, the indexed path scores only LSH candidates.
#[test]
#[ignore = "timing benchmark; run with --ignored --nocapture"]
fn bench_indexed_vs_exhaustive() {
    const N: usize = 400;
    let root = fresh_root("bench");

    // N distinct, realistic-length text leaves.
    let docs: Vec<Vec<u8>> = (0..N)
        .map(|i| {
            format!(
                "document number {i} discusses the migratory patterns of arctic terns \
                 across hemispheres, the metallurgy of early bronze age toolmaking, and \
                 the orbital mechanics of trans neptunian objects in slot number {i}"
            )
            .into_bytes()
        })
        .collect();
    let items: Vec<(&[u8], Modality)> = docs
        .iter()
        .map(|d| (d.as_slice(), Modality::Text))
        .collect();
    let manifest = seal(&root, &items);
    build_all(&manifest, &root.join("cas-root"), SDE).expect("build index");

    // A near-duplicate of leaf 0 (one word changed) → fuzzy hit.
    let probe = b"document number 0 discusses the migratory patterns of arctic terns \
                  across hemispheres, the metallurgy of early bronze age toolmaking, and \
                  the orbital mechanics of trans neptunian objects in SLOT number 0"
        .to_vec();
    let doc = root.join("probe.txt");
    std::fs::write(&doc, &probe).expect("write probe");

    let run = |no_index: bool| {
        let t = std::time::Instant::now();
        let artifact = prove(
            ProofTarget::Document(doc.clone()),
            ManifestSource::Local(manifest.clone()),
            &opts(&root, no_index),
        )
        .expect("probe proves");
        let elapsed = t.elapsed();
        let pred: InclusionProofPredicate =
            serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
        (
            elapsed,
            artifact.confidence,
            serde_json::to_string(&pred.match_evidence).unwrap(),
        )
    };

    let (ex_t, ex_conf, ex_ev) = run(true);
    let (ix_t, ix_conf, ix_ev) = run(false);

    eprintln!("bench over N={N} text leaves:");
    eprintln!("  exhaustive (--no-index): {ex_t:?}");
    eprintln!("  indexed:                 {ix_t:?}");
    eprintln!(
        "  speedup:                 {:.1}x",
        ex_t.as_secs_f64() / ix_t.as_secs_f64().max(1e-9)
    );

    assert_eq!(
        (ex_conf, &ex_ev),
        (ix_conf, &ix_ev),
        "proofs must be identical"
    );
    assert!(
        ix_t < ex_t,
        "indexed query must beat the exhaustive scan ({ix_t:?} vs {ex_t:?})"
    );
}
