//! Integration tests for the S5-D2 E5 fuzzy-match dispatch
//! (`crates/attestrum-prove/src/lib.rs::dispatch_iscc`,
//! `dispatch_perceptual`, `dispatch_document`).
//!
//! E5 uses **CAS re-fingerprint at prove time** — each manifest leaf's
//! bytes are written into a content-addressed store at test setup; the
//! dispatchers fetch + fingerprint them on demand via
//! `attestrum_cas::CasStore`. No precomputed fuzzy-hash sidecar at
//! v0.1.
//!
//! Tests:
//!
//! - `iscc_dispatch_self_match_hits_at_distance_zero`
//! - `perceptual_dispatch_self_match_hits_at_distance_zero`
//! - `document_text_self_match_hits_exact_first`
//! - `document_text_fuzzy_hits_when_not_exact`
//! - `document_text_absent_without_cas_root_is_non_inclusion`
//! - `document_image_self_match_hits_exact_first`
//! - `cas_root_required_for_iscc_dispatch`
//! - `cas_root_required_for_perceptual_dispatch`
//! - `document_unsupported_modality_absent_is_non_inclusion`
//! - `perceptual_dispatch_skips_non_image_leaves`
//!
//! Fixture pattern mirrors `crates/attestrum-prove/tests/exact_match.rs`
//! and `crates/attestrum-attest/tests/cosign_interop.rs` —
//! per-test directories under `CARGO_TARGET_TMPDIR`, atomically
//! counter-suffixed. Manifest entries + corresponding CAS bytes are
//! built side-by-side via the `build_corpus_with_cas` helper.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cas::CasStore;
use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, AttestrumProveError, InclusionProofPredicate, ManifestSource, MatchEvidence,
    PerceptualHashes, ProofKind, ProofTarget, ProveOpts,
};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-prove-e5-{test_name}-{n}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

fn opts_with_cas(root: &Path) -> ProveOpts {
    ProveOpts {
        sign: false,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: Some(root.join("cas-root")),
    }
}

/// Build a manifest whose entries' bytes are also written to a CAS
/// rooted at `<root>/cas-root/`. Each `(bytes, modality)` pair becomes
/// one ManifestEntry whose `document_id` is BLAKE3 of the bytes and
/// whose CAS object contains the same bytes. Returns the manifest path.
fn build_corpus_with_cas(root: &Path, items: &[(&[u8], Modality)]) -> PathBuf {
    let cas = CasStore::new(root.join("cas-root")).expect("create cas");
    let mut entries: Vec<ManifestEntry> = items
        .iter()
        .map(|(bytes, modality)| {
            let h = attestrum_cas::stream_hash(*bytes).expect("hash bytes");
            cas.put(&h.blake3, bytes).expect("cas put");
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
    let manifest_path = root.join("manifest.parquet");
    write_manifest(&manifest_path, &entries).expect("write_manifest");
    manifest_path
}

/// Generate a tiny deterministic PNG fixture (8x8 RGB gradient).
fn make_gradient_png() -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(8, 8, |x, y| {
        Rgb([(x * 32) as u8, (y * 32) as u8, ((x + y) * 16) as u8])
    });
    let mut out = Vec::new();
    let dyn_img = image::DynamicImage::ImageRgb8(buf);
    dyn_img
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

/// Generate a second small PNG (8x8 RGB checkerboard) distinct from
/// the gradient. Used for negative-case Perceptual tests.
fn make_checkerboard_png() -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(8, 8, |x, y| {
        if (x + y) % 2 == 0 {
            Rgb([255, 255, 255])
        } else {
            Rgb([0, 0, 0])
        }
    });
    let mut out = Vec::new();
    let dyn_img = image::DynamicImage::ImageRgb8(buf);
    dyn_img
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

#[test]
fn iscc_dispatch_self_match_hits_at_distance_zero() {
    let root = fresh_root("iscc_self");
    let text_a = b"The quick brown fox jumps over the lazy dog";
    let text_b = b"Wholly different content here, no overlap";
    let manifest =
        build_corpus_with_cas(&root, &[(text_a, Modality::Text), (text_b, Modality::Text)]);

    // Compute the target ISCC by fingerprinting text_a directly, then
    // pass that composite as the ProofTarget. The CAS-iter path will
    // re-fingerprint text_a as one of the leaves and get the same
    // composite → distance 0.
    let fp_opts = attestrum_fingerprint::FingerprintOpts {
        source_date_epoch: 1_700_000_000,
    };
    let bundle = attestrum_fingerprint::fingerprint_text(text_a, &fp_opts).expect("fp text");
    let target_iscc = bundle
        .iscc
        .as_ref()
        .expect("iscc present")
        .composite
        .clone();

    let artifact = prove(
        ProofTarget::Iscc(target_iscc),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("iscc dispatch hits");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    match pred.match_evidence {
        MatchEvidence::Iscc(ev) => assert_eq!(ev.composite_distance, 0),
        other => panic!("expected MatchEvidence::Iscc, got {other:?}"),
    }
    assert_eq!(artifact.confidence, 0.95);
}

#[test]
fn perceptual_dispatch_self_match_hits_at_distance_zero() {
    let root = fresh_root("perc_self");
    let image_a = make_gradient_png();
    let manifest = build_corpus_with_cas(&root, &[(&image_a, Modality::Image)]);

    // Compute target perceptual hashes by fingerprinting the same
    // image; pass them as ProofTarget::Perceptual. CAS-iter
    // re-fingerprints the leaf and computes Hamming over identical
    // pHash + blockhash → distance 0.
    let fp_opts = attestrum_fingerprint::FingerprintOpts {
        source_date_epoch: 1_700_000_000,
    };
    let bundle = attestrum_fingerprint::fingerprint_image(&image_a, &fp_opts).expect("fp image");
    let img = bundle.image.as_ref().expect("image present");
    let mut phash_bytes = [0u8; 8];
    for (i, byte) in phash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img.phash[i * 2..i * 2 + 2], 16).unwrap();
    }
    let mut blockhash_bytes = [0u8; 8];
    for (i, byte) in blockhash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img.blockhash[i * 2..i * 2 + 2], 16).unwrap();
    }
    let target = PerceptualHashes {
        phash: phash_bytes,
        blockhash: blockhash_bytes,
    };

    let artifact = prove(
        ProofTarget::Perceptual(target),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("perceptual dispatch hits");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    match pred.match_evidence {
        MatchEvidence::Perceptual(ev) => {
            assert_eq!(ev.hamming_distance, 0);
            assert_eq!(ev.threshold, 6);
        }
        other => panic!("expected MatchEvidence::Perceptual, got {other:?}"),
    }
    assert_eq!(artifact.confidence, 0.85);
}

/// Like `opts_with_cas` but with no CAS root — the default CLI shape for
/// `attestrum prove <file>` (exact-only, no fuzzy opt-in).
fn opts_no_cas() -> ProveOpts {
    ProveOpts {
        sign: false,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: None,
    }
}

#[test]
fn document_text_self_match_hits_exact_first() {
    // A text document whose RAW bytes are a corpus leaf must prove as a
    // proof-grade exact match (ExactBlake3, 1.00) — even though
    // `fingerprint_text` hashes the *normalized* bytes. `dispatch_document`
    // hashes the document's raw bytes the same way `build` does
    // (`attestrum_cas::stream_hash`), so the exact path fires before any
    // fuzzy fallback. No --cas-root needed: exact match never scans the CAS.
    // This is the grade-wall fix (an exact text file must not be downgraded
    // to a fuzzy 0.95).
    let root = fresh_root("doc_text_exact");
    let text = b"Document text content for inline fingerprint test";
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text)]);

    let doc_path = root.join("input.txt");
    std::fs::write(&doc_path, text).expect("write doc");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_no_cas(),
    )
    .expect("exact text document proves");

    assert_eq!(artifact.kind, ProofKind::Inclusion);
    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    assert_eq!(pred.match_evidence, MatchEvidence::ExactBlake3);
    assert_eq!(artifact.confidence, 1.0);
}

#[test]
fn document_text_fuzzy_hits_when_not_exact() {
    // A text document that is NOT byte-identical to any leaf but normalizes
    // to the same content (case + whitespace differ) misses the raw-bytes
    // exact path and falls through to the fuzzy modes (ISCC then MinHash).
    // Requires --cas-root (fuzzy re-fingerprints leaves from the CAS). This
    // preserves the fuzzy-via-Document coverage that the old self-match test
    // exercised before exact-first landed. (Which fuzzy mode wins depends on
    // the text; both are discovery-grade, < 1.00 — the assertion checks the
    // path resolved to a fuzzy inclusion, not exact and not non-inclusion.)
    let root = fresh_root("doc_text_fuzzy");
    let leaf = b"Hello World";
    let manifest = build_corpus_with_cas(&root, &[(leaf, Modality::Text)]);

    // Different raw bytes, same normalized form (lowercase + collapsed
    // whitespace per the PROTECTED v0.1 text pipeline).
    let probe = b"hello   world";
    let doc_path = root.join("probe.txt");
    std::fs::write(&doc_path, probe).expect("write probe");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("near-match text document hits via fuzzy fallback");

    assert_eq!(artifact.kind, ProofKind::Inclusion);
    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    match pred.match_evidence {
        MatchEvidence::Iscc(_) | MatchEvidence::MinHash(_) => {}
        other => panic!("expected a fuzzy MatchEvidence (Iscc/MinHash), got {other:?}"),
    }
    assert!(
        artifact.confidence < 1.0,
        "fuzzy match must be discovery-grade (< 1.00), got {}",
        artifact.confidence
    );
}

#[test]
fn document_text_absent_without_cas_root_is_non_inclusion() {
    // A text document NOT in the corpus, proved by path with no --cas-root
    // (the default CLI shape): the exact raw bytes aren't a leaf and no
    // fuzzy scan is requested, so the dispatcher emits a proof-grade
    // NON-INCLUSION (1.00) rather than the old confusing
    // "manifest format invalid: fuzzy non-inclusion is v0.2 work" error.
    let root = fresh_root("doc_text_absent");
    let leaf = b"this text is in the corpus";
    let manifest = build_corpus_with_cas(&root, &[(leaf, Modality::Text)]);

    let doc_path = root.join("absent.txt");
    std::fs::write(&doc_path, b"this text is NOT in the corpus").expect("write doc");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_no_cas(),
    )
    .expect("absent document yields proof-grade non-inclusion");

    assert_eq!(artifact.kind, ProofKind::NonInclusion);
    assert_eq!(artifact.confidence, 1.0);
}

#[test]
fn document_image_self_match_hits_exact_first() {
    let root = fresh_root("doc_image");
    let img = make_gradient_png();
    let manifest = build_corpus_with_cas(&root, &[(&img, Modality::Image)]);

    let doc_path = root.join("input.png");
    std::fs::write(&doc_path, &img).expect("write doc");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("document dispatch hits");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    assert_eq!(pred.match_evidence, MatchEvidence::ExactBlake3);
    assert_eq!(artifact.confidence, 1.0);
}

#[test]
fn cas_root_required_for_iscc_dispatch() {
    let root = fresh_root("iscc_no_cas");
    let text = b"any content";
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text)]);

    let mut opts = opts_with_cas(&root);
    opts.cas_root = None;

    let err = prove(
        ProofTarget::Iscc(String::from("ISCC:KACT4EBWK27737D2")),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect_err("cas_root required for fuzzy dispatch");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("cas_root required"),
                "error message must mention cas_root: got {msg:?}"
            );
        }
        other => panic!("expected InvalidManifest, got {other:?}"),
    }
}

#[test]
fn cas_root_required_for_perceptual_dispatch() {
    let root = fresh_root("perc_no_cas");
    let img = make_gradient_png();
    let manifest = build_corpus_with_cas(&root, &[(&img, Modality::Image)]);

    let mut opts = opts_with_cas(&root);
    opts.cas_root = None;

    let err = prove(
        ProofTarget::Perceptual(PerceptualHashes {
            phash: [0u8; 8],
            blockhash: [0u8; 8],
        }),
        ManifestSource::Local(manifest),
        &opts,
    )
    .expect_err("cas_root required for fuzzy dispatch");

    assert!(matches!(err, AttestrumProveError::InvalidManifest(_)));
}

#[test]
fn document_unsupported_modality_absent_is_non_inclusion() {
    let root = fresh_root("doc_unsupported");
    let text = b"any text leaf";
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text)]);

    // Random binary bytes — not valid UTF-8 and not a recognized image
    // format, so no fuzzy fingerprint can be computed. But exact absence
    // doesn't depend on modality: the raw bytes aren't a leaf, so the
    // dispatcher emits a proof-grade non-inclusion. (Pre-exact-first this
    // errored with "unsupported document modality" — a worse answer than
    // the true "this exact document is not in the corpus".)
    let garbage = [0xffu8, 0xfe, 0x00, 0x01, 0xff, 0xff, 0x00, 0xff];
    let doc_path = root.join("garbage.bin");
    std::fs::write(&doc_path, garbage).expect("write garbage");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("unsupported-modality absent document yields non-inclusion");

    assert_eq!(artifact.kind, ProofKind::NonInclusion);
    assert_eq!(artifact.confidence, 1.0);
}

#[test]
fn perceptual_dispatch_skips_non_image_leaves() {
    let root = fresh_root("perc_skip_text");
    let text = b"This is a text leaf that perceptual scan must skip silently";
    let img = make_gradient_png();
    // Mixed manifest: one text leaf + one image leaf. Perceptual
    // dispatch must skip the text leaf without panic and hit the
    // image leaf.
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text), (&img, Modality::Image)]);

    // Build the same perceptual target as the image leaf would produce.
    let fp_opts = attestrum_fingerprint::FingerprintOpts {
        source_date_epoch: 1_700_000_000,
    };
    let bundle = attestrum_fingerprint::fingerprint_image(&img, &fp_opts).expect("fp image");
    let img_fp = bundle.image.as_ref().unwrap();
    let mut phash_bytes = [0u8; 8];
    for (i, byte) in phash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img_fp.phash[i * 2..i * 2 + 2], 16).unwrap();
    }
    let mut blockhash_bytes = [0u8; 8];
    for (i, byte) in blockhash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img_fp.blockhash[i * 2..i * 2 + 2], 16).unwrap();
    }

    let artifact = prove(
        ProofTarget::Perceptual(PerceptualHashes {
            phash: phash_bytes,
            blockhash: blockhash_bytes,
        }),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("image hit despite text leaf present");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    assert!(matches!(pred.match_evidence, MatchEvidence::Perceptual(_)));
}

#[test]
fn perceptual_dispatch_distinguishes_distinct_images() {
    // Both leaves are images; query for the gradient — should hit the
    // gradient leaf, not the checkerboard. Confirms the dispatcher
    // actually picks the closest leaf rather than always returning
    // index 0.
    let root = fresh_root("perc_distinguish");
    let gradient = make_gradient_png();
    let checker = make_checkerboard_png();
    let manifest = build_corpus_with_cas(
        &root,
        &[(&checker, Modality::Image), (&gradient, Modality::Image)],
    );

    let fp_opts = attestrum_fingerprint::FingerprintOpts {
        source_date_epoch: 1_700_000_000,
    };
    let bundle =
        attestrum_fingerprint::fingerprint_image(&gradient, &fp_opts).expect("fp gradient");
    let img = bundle.image.as_ref().unwrap();
    let mut phash_bytes = [0u8; 8];
    for (i, byte) in phash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img.phash[i * 2..i * 2 + 2], 16).unwrap();
    }
    let mut blockhash_bytes = [0u8; 8];
    for (i, byte) in blockhash_bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&img.blockhash[i * 2..i * 2 + 2], 16).unwrap();
    }

    let artifact = prove(
        ProofTarget::Perceptual(PerceptualHashes {
            phash: phash_bytes,
            blockhash: blockhash_bytes,
        }),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("hit");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    // gradient bytes hash to a specific BLAKE3; assert leaf_hash matches.
    let expected_b3 =
        attestrum_core::hex::encode_32(&attestrum_cas::stream_hash(&gradient[..]).unwrap().blake3);
    assert_eq!(pred.leaf_hash, expected_b3);
}
