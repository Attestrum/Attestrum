//! Stage A1 — filesystem integration tests for `StaticBundleTarget::publish()`.
//!
//! Unlike the HF target (which talks to a `wiremock` mock Hub), the static
//! target writes to local disk, so these tests are plain synchronous `#[test]`s
//! backed by `tempfile::TempDir` fixture roots (auto-cleaned on drop). They
//! prove the behavioral contract from `docs/diagrams/overview/static-publish.md`:
//!
//! 1. the canonical six files land at the right relative paths;
//! 2. the three rendered artifacts are byte-equal to the `attestrum_emit::render_*`
//!    outputs (proves publish actually renders, not a stale string);
//! 3. the three sealed inputs are copied verbatim;
//! 4. the receipt carries `target=static`, `commit_oid=None`, and `file://` URLs;
//! 5. an absent sealed input errors with `BundleMissing` before any write;
//! 6. output is byte-stable across two independent out-dirs;
//! 7. re-publishing into the same dir is idempotent (overwrite);
//! 8. `plan.extras` are copied to their repo-relative destinations.

use std::path::Path;

use attestrum_publish::{
    AttestrumPublishError, CroissantPlan, DatasetCardPlan, ManifestStats, PublishPlan,
    PublishReceipt, PublishTarget, StaticBundleTarget, VerifyHtmlPlan,
};
use tempfile::TempDir;

const TEST_REPO: &str = "test-org/test-dataset";

// Raw fixture bytes for the three sealed inputs the static target copies
// verbatim. Asserting against these constants proves the copy is byte-exact.
const MANIFEST_BYTES: &[u8] = b"PAR1\x00\x00fake parquet payload";
const MERKLE_BYTES: &[u8] = &[0xAB_u8; 32];
const BUNDLE_BYTES: &[u8] = b"{\"fake\":\"sigstore-bundle\"}";

/// The six canonical repo-relative paths every static publish must produce.
const CANONICAL_PATHS: [&str; 6] = [
    "README.md",
    "croissant.json",
    "attestrum/manifest.parquet",
    "attestrum/merkle.root",
    "attestrum/bundle.sigstore.json",
    "attestrum/verify.html",
];

/// Write the three sealed-input fixtures into `dir` and return a `PublishPlan`
/// pointing at them. The caller keeps the owning `TempDir` alive until after
/// `publish()` runs.
fn write_fixture_plan(dir: &Path) -> PublishPlan {
    std::fs::write(dir.join("manifest.parquet"), MANIFEST_BYTES).expect("write manifest fixture");
    std::fs::write(dir.join("merkle.root"), MERKLE_BYTES).expect("write merkle.root fixture");
    std::fs::write(dir.join("bundle.sigstore.json"), BUNDLE_BYTES).expect("write bundle fixture");
    let stats = ManifestStats {
        leaf_count: 5,
        total_bytes: 1024,
    };
    PublishPlan {
        manifest_path: dir.join("manifest.parquet"),
        bundle_path: dir.join("bundle.sigstore.json"),
        merkle_root_path: dir.join("merkle.root"),
        croissant_plan: CroissantPlan {
            dataset_name: TEST_REPO.to_string(),
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
            merkle_root_path_in_repo: "attestrum/merkle.root".to_string(),
            manifest_stats: stats,
            source_date_epoch: 1_700_000_000,
            license_spdx: Some("Apache-2.0".to_string()),
            version: Some("1.0.0".to_string()),
            cite_as: None,
        },
        dataset_card_plan: DatasetCardPlan {
            pretty_name: "Test Dataset".to_string(),
            license_spdx: "Apache-2.0".to_string(),
            language: vec!["en".to_string()],
            task_categories: vec!["text-generation".to_string()],
            size_category: "n<1K".to_string(),
            tags: vec!["example".to_string()],
            dataset_name: TEST_REPO.to_string(),
            manifest_stats: stats,
            // Static-target convention: relative, re-hostable. (Only affects
            // the rendered README content; the receipt URL is built separately.)
            verify_url: "attestrum/verify.html".to_string(),
        },
        verify_html_plan: VerifyHtmlPlan {
            dataset_name: TEST_REPO.to_string(),
            certificate_identity:
                "https://github.com/test-org/test-dataset/.github/workflows/build.yml@refs/heads/main"
                    .to_string(),
            certificate_oidc_issuer: "https://token.actions.githubusercontent.com".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
            manifest_stats: stats,
        },
        extras: Vec::new(),
    }
}

#[test]
fn publish_writes_the_canonical_six_files() {
    let fixtures = TempDir::new().expect("fixtures tempdir");
    let out = TempDir::new().expect("out tempdir");
    let plan = write_fixture_plan(fixtures.path());
    let target = StaticBundleTarget {
        out_dir: out.path().join("bundle"),
    };

    let receipt: PublishReceipt = target
        .publish(&plan)
        .expect("static publish should succeed");
    let root = &target.out_dir;

    // All six canonical files present at the expected relative paths.
    for rel in CANONICAL_PATHS {
        assert!(root.join(rel).is_file(), "missing expected artifact: {rel}");
    }

    // Rendered artifacts are byte-equal to the emit renderers on the same plan
    // (proves publish actually rendered, not a stale fixture string).
    let expected_readme = attestrum_emit::render_readme(&plan.dataset_card_plan).expect("readme");
    assert_eq!(
        std::fs::read(root.join("README.md")).expect("read README.md"),
        expected_readme.into_bytes(),
    );
    let expected_croissant =
        attestrum_emit::render_croissant(&plan.croissant_plan).expect("croissant");
    assert_eq!(
        std::fs::read(root.join("croissant.json")).expect("read croissant.json"),
        expected_croissant.into_bytes(),
    );
    let expected_verify =
        attestrum_emit::render_verify_html_stub(&plan.verify_html_plan).expect("verify");
    assert_eq!(
        std::fs::read(root.join("attestrum/verify.html")).expect("read verify.html"),
        expected_verify.into_bytes(),
    );

    // Sealed inputs copied verbatim.
    assert_eq!(
        std::fs::read(root.join("attestrum/manifest.parquet")).expect("read manifest"),
        MANIFEST_BYTES,
    );
    assert_eq!(
        std::fs::read(root.join("attestrum/merkle.root")).expect("read merkle.root"),
        MERKLE_BYTES,
    );
    assert_eq!(
        std::fs::read(root.join("attestrum/bundle.sigstore.json")).expect("read bundle"),
        BUNDLE_BYTES,
    );

    // Receipt contract.
    assert_eq!(receipt.target, "static");
    assert_eq!(receipt.commit_oid, None);
    assert!(
        receipt.dataset_url.starts_with("file://"),
        "dataset_url must be a file:// URL, got {}",
        receipt.dataset_url
    );
    assert!(
        receipt.verify_url.starts_with("file://")
            && receipt.verify_url.ends_with("/attestrum/verify.html"),
        "verify_url must be a file:// URL ending in the verify page, got {}",
        receipt.verify_url
    );
}

#[test]
fn publish_missing_input_errors_bundle_missing_before_any_write() {
    let fixtures = TempDir::new().expect("fixtures tempdir");
    let out = TempDir::new().expect("out tempdir");
    let mut plan = write_fixture_plan(fixtures.path());
    // Point the bundle at a path that doesn't exist.
    plan.bundle_path = fixtures.path().join("does-not-exist.sigstore.json");
    let out_root = out.path().join("bundle");
    let target = StaticBundleTarget {
        out_dir: out_root.clone(),
    };

    let err = target.publish(&plan).expect_err("absent bundle must error");
    assert!(
        matches!(err, AttestrumPublishError::BundleMissing(_)),
        "expected BundleMissing, got {err:?}"
    );
    // Pre-flight validation runs before any directory is created.
    assert!(
        !out_root.exists(),
        "out_dir must not be created when an input is missing"
    );
}

#[test]
fn publish_is_byte_stable_across_independent_out_dirs() {
    let fixtures = TempDir::new().expect("fixtures tempdir");
    let out_a = TempDir::new().expect("out a");
    let out_b = TempDir::new().expect("out b");
    let plan = write_fixture_plan(fixtures.path());

    let target_a = StaticBundleTarget {
        out_dir: out_a.path().join("bundle"),
    };
    let target_b = StaticBundleTarget {
        out_dir: out_b.path().join("bundle"),
    };
    target_a.publish(&plan).expect("publish a");
    target_b.publish(&plan).expect("publish b");

    for rel in CANONICAL_PATHS {
        let a = std::fs::read(target_a.out_dir.join(rel)).expect("read a");
        let b = std::fs::read(target_b.out_dir.join(rel)).expect("read b");
        assert_eq!(a, b, "byte mismatch across out-dirs for {rel}");
    }
}

#[test]
fn publish_is_idempotent_on_re_publish() {
    let fixtures = TempDir::new().expect("fixtures tempdir");
    let out = TempDir::new().expect("out tempdir");
    let plan = write_fixture_plan(fixtures.path());
    let target = StaticBundleTarget {
        out_dir: out.path().join("bundle"),
    };

    target.publish(&plan).expect("first publish");
    let first: Vec<Vec<u8>> = CANONICAL_PATHS
        .iter()
        .map(|rel| std::fs::read(target.out_dir.join(rel)).expect("read first"))
        .collect();

    // Re-publishing into a non-empty dir must succeed and overwrite to the
    // same bytes.
    target.publish(&plan).expect("re-publish into existing dir");
    for (i, rel) in CANONICAL_PATHS.iter().enumerate() {
        let second = std::fs::read(target.out_dir.join(rel)).expect("read second");
        assert_eq!(second, first[i], "re-publish changed {rel}");
    }
}

#[test]
fn publish_copies_extras_to_repo_relative_paths() {
    let fixtures = TempDir::new().expect("fixtures tempdir");
    let out = TempDir::new().expect("out tempdir");
    let mut plan = write_fixture_plan(fixtures.path());

    let extra_src = fixtures.path().join("license-inventory.json");
    let extra_bytes = b"{\"licenses\":[]}";
    std::fs::write(&extra_src, extra_bytes).expect("write extra fixture");
    plan.extras = vec![(extra_src, "attestrum/license-inventory.json".to_string())];

    let target = StaticBundleTarget {
        out_dir: out.path().join("bundle"),
    };
    target.publish(&plan).expect("publish with extras");

    assert_eq!(
        std::fs::read(target.out_dir.join("attestrum/license-inventory.json")).expect("read extra"),
        extra_bytes,
    );
}
