//! Integration tests for the S5-D2 E7 alternate manifest sources
//! (`HuggingFace` + `Url`) with workspace-cached fetch.
//!
//! Pins the public contract enumerated in the planning diagram at
//! `docs/diagrams/sprint-5/prove-pipeline.md` and the E7 plan file at
//! `~/.claude/plans/s5-d2-e7-hf-url-manifest-sources.md`:
//!
//! - HF Hub resolve URL construction (default `main` revision when
//!   `revision: None`, explicit pin when `Some("v1.0")`).
//! - Workspace-cache key disambiguates source-type (HF vs URL),
//!   repo identity, and revision pin.
//! - Cache HIT path (pre-populated cache file) skips the network
//!   entirely and reuses the byte-identical fixture manifest.
//! - URL scheme guard rejects non-http(s) inputs with a clear
//!   `SourceUnreachable` message.
//! - `Local` source pass-through is unchanged.
//! - Real HF + URL fetches are gated behind env vars + `#[ignore]`
//!   (mirrors `tests/sign_integration.rs::signed_prove_emits_verifiable_bundle`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{
    prove, AttestrumProveError, ManifestSource, ProofKind, ProofTarget, ProveOpts,
};

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-prove-e7-{test_name}-{n}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

fn digest(b: u8) -> [u8; 32] {
    [b; 32]
}

fn sample_entry(doc_byte: u8) -> ManifestEntry {
    ManifestEntry {
        document_id: digest(doc_byte),
        sha256: digest(doc_byte ^ 0xff),
        size_bytes: u64::from(doc_byte) * 100,
        modality: Modality::Text,
        mime_type: Some("text/plain".into()),
        source_url: Some(format!("file:///docs/doc-{doc_byte:02x}.txt")),
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
}

fn build_test_manifest(root: &Path, doc_bytes: &[u8]) -> PathBuf {
    let mut entries: Vec<ManifestEntry> = doc_bytes.iter().map(|b| sample_entry(*b)).collect();
    assign_input_ordinals(&mut entries);
    sort_entries(&mut entries);
    assign_occurrence_indices(&mut entries);
    let manifest_path = root.join("manifest.parquet");
    write_manifest(&manifest_path, &entries).expect("write_manifest");
    manifest_path
}

fn opts_with_workspace(workspace: &Path) -> ProveOpts {
    ProveOpts {
        sign: false,
        source_date_epoch: 1_700_000_000,
        oidc_id_token: None,
        workspace: Some(workspace.to_path_buf()),
        corpus_bundle_path: None,
        cas_root: None,
        no_index: false,
    }
}

// ============================================================================
// Cache HIT path — end-to-end via `prove()` against a pre-populated cache.
// ============================================================================
//
// We can't drive the helper functions directly (`cache_key_for_source`,
// `build_hf_url`, etc. are private). Instead we exercise the public
// `prove()` entry point + observe the behaviorally significant fact:
// when a cache file exists at the expected location, the network is
// never touched (the test runs offline). The "expected location" is
// derived by inspection of the implementation's algorithm — the test
// suite would fail if that algorithm changed in a breaking way, which
// is the right cost.

/// Re-implement the cache-key derivation locally so we can pre-populate
/// the cache directory the implementation will check. If the
/// implementation's key derivation changes, this helper must be updated
/// in lockstep — the integration test then catches stale fixtures.
/// Kept in sync with `cache_key_for_source` in `src/lib.rs`.
fn expected_cache_key(source: &ManifestSource) -> String {
    use sha2::{Digest, Sha256};
    let descriptor = match source {
        ManifestSource::Local(_) => unreachable!("Local has no cache key"),
        ManifestSource::HuggingFace { repo, revision } => {
            let rev = revision.as_deref().unwrap_or("main");
            format!("huggingface:{repo}@{rev}")
        }
        ManifestSource::Url(url) => format!("url:{url}"),
    };
    let digest: [u8; 32] = Sha256::digest(descriptor.as_bytes()).into();
    attestrum_core::hex::encode_32(&digest)
}

fn populate_cache(workspace: &Path, source: &ManifestSource, canned_manifest: &Path) -> PathBuf {
    let cache_dir = workspace
        .join("prove")
        .join("manifest-cache")
        .join(expected_cache_key(source));
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");
    let cache_path = cache_dir.join("manifest.parquet");
    std::fs::copy(canned_manifest, &cache_path).expect("seed cache file");
    cache_path
}

#[test]
fn huggingface_source_with_cache_hit_skips_network() {
    let root = fresh_root("hf_cache_hit");
    let workspace = root.join("workspace");
    let canned = build_test_manifest(&root, &[0x10, 0x20, 0x30]);
    let source = ManifestSource::HuggingFace {
        repo: "attestrum-test/sample".into(),
        revision: Some("v1.0".into()),
    };
    let _cache_path = populate_cache(&workspace, &source, &canned);

    // Target matches leaf 1 (digest 0x20) → InclusionProof via cache hit.
    let artifact = prove(
        ProofTarget::Blake3(digest(0x20)),
        source,
        &opts_with_workspace(&workspace),
    )
    .expect("cache-hit path returns Ok without any network call");

    assert_eq!(artifact.kind, ProofKind::Inclusion);
    assert_eq!(artifact.confidence, 1.0);
}

#[test]
fn url_source_with_cache_hit_skips_network() {
    let root = fresh_root("url_cache_hit");
    let workspace = root.join("workspace");
    let canned = build_test_manifest(&root, &[0x10, 0x20]);
    let source = ManifestSource::Url("https://example.invalid/m.parquet".into());
    populate_cache(&workspace, &source, &canned);

    // Target NOT in manifest → NonInclusion via cache hit (E6 + E7 chain).
    let artifact = prove(
        ProofTarget::Blake3(digest(0xab)),
        source,
        &opts_with_workspace(&workspace),
    )
    .expect("cache-hit path returns Ok without any network call");

    assert_eq!(artifact.kind, ProofKind::NonInclusion);
}

#[test]
fn cache_key_distinguishes_hf_and_url_with_same_string() {
    let hf = ManifestSource::HuggingFace {
        repo: "a/b".into(),
        revision: None,
    };
    let url = ManifestSource::Url("a/b".into());
    assert_ne!(
        expected_cache_key(&hf),
        expected_cache_key(&url),
        "huggingface: prefix vs url: prefix must disambiguate"
    );
}

#[test]
fn cache_key_distinguishes_revisions() {
    let v1 = ManifestSource::HuggingFace {
        repo: "a/b".into(),
        revision: Some("v1".into()),
    };
    let v2 = ManifestSource::HuggingFace {
        repo: "a/b".into(),
        revision: Some("v2".into()),
    };
    assert_ne!(
        expected_cache_key(&v1),
        expected_cache_key(&v2),
        "revision pin must affect cache key"
    );
}

#[test]
fn cache_key_none_revision_equals_main_revision() {
    let none = ManifestSource::HuggingFace {
        repo: "a/b".into(),
        revision: None,
    };
    let main = ManifestSource::HuggingFace {
        repo: "a/b".into(),
        revision: Some("main".into()),
    };
    assert_eq!(
        expected_cache_key(&none),
        expected_cache_key(&main),
        "None revision must default to \"main\" per the documented convention"
    );
}

#[test]
fn cache_key_stable_for_same_source() {
    let s = ManifestSource::HuggingFace {
        repo: "allenai/c4".into(),
        revision: Some("0.1.0".into()),
    };
    let a = expected_cache_key(&s);
    let b = expected_cache_key(&s);
    assert_eq!(a, b);
    assert_eq!(a.len(), 64, "sha256 hex is 64 characters");
}

#[test]
fn url_scheme_must_be_http_or_https() {
    let root = fresh_root("url_scheme_guard");
    let workspace = root.join("workspace");

    let err = prove(
        ProofTarget::Blake3(digest(0x10)),
        ManifestSource::Url("ftp://example.invalid/m.parquet".into()),
        &opts_with_workspace(&workspace),
    )
    .expect_err("ftp URL must be rejected before any network call");

    match err {
        AttestrumProveError::SourceUnreachable(msg) => {
            assert!(
                msg.contains("must start with http:// or https://"),
                "expected scheme-guard error, got: {msg}"
            );
        }
        other => panic!("expected SourceUnreachable, got: {other:?}"),
    }
}

#[test]
fn local_source_does_not_touch_cache() {
    let root = fresh_root("local_passthrough");
    let workspace = root.join("workspace");
    let manifest = build_test_manifest(&root, &[0x10, 0x20]);

    let artifact = prove(
        ProofTarget::Blake3(digest(0x10)),
        ManifestSource::Local(manifest),
        &opts_with_workspace(&workspace),
    )
    .expect("Local source resolves to passed path without cache interaction");

    assert_eq!(artifact.kind, ProofKind::Inclusion);
    // Workspace dir might not even exist (Local doesn't trigger cache-dir creation).
    let cache_root = workspace.join("prove").join("manifest-cache");
    assert!(
        !cache_root.exists(),
        "Local source must NOT create the manifest-cache directory"
    );
}

#[test]
fn cache_path_layout_is_workspace_prove_manifest_cache_key() {
    // Pin the on-disk layout the planning diagram + CHANGELOG document.
    // Layout: <workspace>/prove/manifest-cache/<sha256-of-source-key>/manifest.parquet
    let root = fresh_root("cache_layout");
    let workspace = root.join("workspace");
    let canned = build_test_manifest(&root, &[0x10]);
    let source = ManifestSource::HuggingFace {
        repo: "org/name".into(),
        revision: Some("v1.0".into()),
    };
    let cache_path = populate_cache(&workspace, &source, &canned);

    let expected_dir = workspace
        .join("prove")
        .join("manifest-cache")
        .join(expected_cache_key(&source));
    let expected_path = expected_dir.join("manifest.parquet");
    assert_eq!(cache_path, expected_path);
    assert!(cache_path.is_file());
}

// ============================================================================
// Real-network integration tests (env-gated, silent skip when unset)
// ============================================================================
//
// Mirrors `tests/sign_integration.rs::signed_prove_emits_verifiable_bundle`:
// `#[ignore]` so default `cargo test` skips them; opt-in via
// `cargo test -- --ignored`; body silently returns when the env var is
// missing so a CI matrix without the credential doesn't fail.

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[test]
#[ignore = "requires ATTESTRUM_HF_TEST_DATASET=<owner/repo[@revision]> + network to huggingface.co"]
fn hf_dataset_fetch_smoke() {
    let dataset = match env_var("ATTESTRUM_HF_TEST_DATASET") {
        Some(d) => d,
        None => return,
    };
    let (repo, revision) = match dataset.split_once('@') {
        Some((r, rev)) => (r.to_string(), Some(rev.to_string())),
        None => (dataset, None),
    };
    let root = fresh_root("hf_real_fetch");
    let workspace = root.join("workspace");
    let source = ManifestSource::HuggingFace { repo, revision };

    // First call: cache miss → fetches over the network.
    let artifact1 = prove(
        ProofTarget::Blake3(digest(0xab)),
        source.clone(),
        &opts_with_workspace(&workspace),
    )
    .expect("first prove() against real HF dataset");
    // Cache file should exist after the first call.
    let cache_dir = workspace
        .join("prove")
        .join("manifest-cache")
        .join(expected_cache_key(&source));
    assert!(
        cache_dir.join("manifest.parquet").is_file(),
        "cache file must be written after first fetch"
    );
    // Second call: cache HIT (no network).
    let artifact2 = prove(
        ProofTarget::Blake3(digest(0xab)),
        source,
        &opts_with_workspace(&workspace),
    )
    .expect("second prove() hits cache");
    // Same target against same manifest → same proof kind (inclusion or
    // non-inclusion depending on the dataset; both are valid here).
    assert_eq!(artifact1.kind, artifact2.kind);
}

#[test]
#[ignore = "requires ATTESTRUM_URL_TEST_MANIFEST=<https://...> + network access"]
fn url_fetch_smoke() {
    let url = match env_var("ATTESTRUM_URL_TEST_MANIFEST") {
        Some(u) => u,
        None => return,
    };
    let root = fresh_root("url_real_fetch");
    let workspace = root.join("workspace");
    let source = ManifestSource::Url(url);

    let artifact = prove(
        ProofTarget::Blake3(digest(0xab)),
        source.clone(),
        &opts_with_workspace(&workspace),
    )
    .expect("URL fetch against real network");
    let cache_dir = workspace
        .join("prove")
        .join("manifest-cache")
        .join(expected_cache_key(&source));
    assert!(cache_dir.join("manifest.parquet").is_file());
    // Either kind is valid here — the contract under test is the fetch +
    // cache-write, not the inclusion outcome.
    let _ = artifact.kind;
}

#[test]
#[ignore = "requires ATTESTRUM_HF_PRIVATE_DATASET=<owner/repo> + (optionally) HF_TOKEN"]
fn hf_private_dataset_auth_error_includes_hint() {
    let repo = match env_var("ATTESTRUM_HF_PRIVATE_DATASET") {
        Some(d) => d,
        None => return,
    };

    let root = fresh_root("hf_private_unauth");
    let workspace = root.join("workspace");
    let source = ManifestSource::HuggingFace {
        repo,
        revision: None,
    };

    // Force-unset HF_TOKEN for this call to exercise the unauthenticated
    // request path. We can't safely mutate process env in parallel tests,
    // so just check whatever the ambient state is.
    if std::env::var("HF_TOKEN").is_ok() {
        // Token is set → expect success (no hint).
        let _ = prove(
            ProofTarget::Blake3(digest(0xab)),
            source,
            &opts_with_workspace(&workspace),
        )
        .expect("HF_TOKEN set → private dataset fetch should succeed");
        return;
    }

    // No token → expect a SourceUnreachable error whose message
    // includes the "set HF_TOKEN env var" hint.
    let err = prove(
        ProofTarget::Blake3(digest(0xab)),
        source,
        &opts_with_workspace(&workspace),
    )
    .expect_err("private dataset without HF_TOKEN must error");
    match err {
        AttestrumProveError::SourceUnreachable(msg) => {
            assert!(
                msg.contains("HF_TOKEN"),
                "auth-error message must hint at HF_TOKEN, got: {msg}"
            );
        }
        other => panic!("expected SourceUnreachable, got: {other:?}"),
    }
}
