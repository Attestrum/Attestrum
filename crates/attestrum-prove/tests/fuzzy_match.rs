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
//! - `document_image_self_match_hits_exact_first`
//! - `cas_root_required_for_iscc_dispatch`
//! - `cas_root_required_for_perceptual_dispatch`
//! - `document_unsupported_modality_returns_invalid_manifest`
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
    PerceptualHashes, ProofTarget, ProveOpts,
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

#[test]
fn document_text_self_match_hits_iscc_after_exact_fallthrough() {
    // Text fingerprints hash the **normalized** bytes (NFC + lowercase
    // + whitespace collapse) per the PROTECTED `attestrum-fingerprint`
    // v0.1 pipeline. The manifest stores **raw-bytes** BLAKE3 (via
    // `stream_hash` in the build pipeline). So for text Documents the
    // exact-BLAKE3 / exact-SHA-256 paths fundamentally cannot match
    // (different inputs to the hash). The multi-mode dispatcher tries
    // exact first, finds nothing, falls through to ISCC — which DOES
    // match because `fingerprint_text` of identical raw bytes yields
    // identical ISCC composite codes (deterministic). For text
    // Documents the realistic match mode is ISCC (0.95) or MinHash
    // (0.80), not exact (1.00).
    //
    // For image Documents both BLAKE3 inputs are raw bytes, so exact
    // works there (see `document_image_self_match_hits_exact_first`).
    let root = fresh_root("doc_text");
    let text = b"Document text content for inline fingerprint test";
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text)]);

    let doc_path = root.join("input.txt");
    std::fs::write(&doc_path, text).expect("write doc");

    let artifact = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect("document dispatch hits via ISCC fallback");

    let pred: InclusionProofPredicate =
        serde_json::from_value(artifact.statement.predicate.clone()).expect("parse");
    match pred.match_evidence {
        MatchEvidence::Iscc(ev) => assert_eq!(ev.composite_distance, 0),
        other => panic!("expected MatchEvidence::Iscc (text exact paths can't hit), got {other:?}"),
    }
    assert_eq!(artifact.confidence, 0.95);
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
fn document_unsupported_modality_returns_invalid_manifest() {
    let root = fresh_root("doc_unsupported");
    let text = b"any text leaf";
    let manifest = build_corpus_with_cas(&root, &[(text, Modality::Text)]);

    // Random binary bytes — not valid UTF-8 and not a recognized
    // image format. Document MIME detection should fail with
    // InvalidManifest.
    let garbage = [0xffu8, 0xfe, 0x00, 0x01, 0xff, 0xff, 0x00, 0xff];
    let doc_path = root.join("garbage.bin");
    std::fs::write(&doc_path, garbage).expect("write garbage");

    let err = prove(
        ProofTarget::Document(doc_path),
        ManifestSource::Local(manifest),
        &opts_with_cas(&root),
    )
    .expect_err("unsupported modality should error");

    match err {
        AttestrumProveError::InvalidManifest(msg) => {
            assert!(
                msg.contains("unsupported document modality"),
                "error must explain the modality cap: got {msg:?}"
            );
        }
        other => panic!("expected InvalidManifest, got {other:?}"),
    }
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
