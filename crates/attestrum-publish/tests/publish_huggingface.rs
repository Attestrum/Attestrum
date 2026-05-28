//! S5-D3 E3 — wiremock-backed integration tests for `HuggingFaceTarget::publish()`.
//!
//! Each test spins its own `wiremock::MockServer` on a dedicated tokio
//! runtime (clean isolation, sub-50ms startup) and points
//! `HuggingFaceTarget::new_with_endpoint()` at the mock's `uri()` instead of
//! `https://huggingface.co`. The 5 tests cover:
//!
//! 1. happy path — 6-op commit round-trips, receipt fields are correct.
//! 2. 401 on `/api/repos/create` → `AttestrumPublishError::Auth`.
//! 3. 429 on `/api/repos/create` → `AttestrumPublishError::Quota`.
//! 4. 409 on `/api/repos/create` with `exist_ok=true` → idempotent (commit completes).
//! 5. connection refused (mock server torn down before publish) → `Network`.
//!
//! hf-hub's commit flow is `create_repo → preupload → create_commit` (three
//! HTTP calls per publish). The preupload classifies each file as `"regular"`
//! or `"lfs"`; for our <1MB fixture files all return `"regular"`, so no LFS
//! upload step fires.
//!
//! ## Runtime architecture
//!
//! hf-hub's `HFClientSync` wraps an async client behind a private tokio
//! runtime and calls `runtime.block_on(...)` for each method. That panics
//! with "Cannot start a runtime from within a runtime" if invoked from inside
//! an existing tokio runtime (which is what `#[tokio::test]` provides). The
//! pattern below sidesteps this:
//!
//! - `MockServer::start().await` is driven by a dedicated `tokio::runtime::
//!   Runtime` created with `Builder::new_multi_thread()`. The server's
//!   listener task runs on that runtime's worker threads in the background.
//! - The test body runs as a plain `#[test]` (synchronous) on the test
//!   harness thread, which has no tokio context. `target.publish(&plan)`
//!   runs there directly — hf-hub's `block_on` finds no active runtime and
//!   installs its own.
//! - The `MockServer` binding stays alive for the duration of the test (its
//!   `Drop` shuts down the listener), so HTTP requests from `publish()`
//!   reach the listener task on the side runtime.

use std::path::Path;

use attestrum_publish::{
    AttestrumPublishError, HuggingFaceTarget, PublishPlan, PublishReceipt, PublishTarget,
};
use serde_json::json;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_REPO: &str = "test-org/test-dataset";
const TEST_BRANCH: &str = "main";
const TEST_COMMIT_OID: &str = "abc123def4567890abc123def4567890abc123de";

/// Build a multi-threaded tokio runtime for the side process driving the
/// MockServer's listener. Each test owns one; dropped at test end.
fn server_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("build wiremock runtime")
}

/// Build a `PublishPlan` backed by tempdir-rooted fixture files. The caller
/// must keep the returned `TempDir` alive until after `publish()` runs — its
/// `Drop` deletes the fixtures.
fn write_fixture_plan(dir: &Path) -> PublishPlan {
    std::fs::write(dir.join("manifest.parquet"), b"PAR1\x00\x00fake parquet")
        .expect("write manifest fixture");
    std::fs::write(dir.join("merkle.root"), [0xAB_u8; 32]).expect("write merkle.root fixture");
    std::fs::write(dir.join("bundle.sigstore.json"), b"{\"fake\":\"bundle\"}")
        .expect("write bundle fixture");
    PublishPlan {
        manifest_path: dir.join("manifest.parquet"),
        bundle_path: dir.join("bundle.sigstore.json"),
        merkle_root_path: dir.join("merkle.root"),
        croissant: json!({"@context": "https://schema.org/", "@type": "sc:Dataset"}),
        readme: "# Test Dataset\n".to_string(),
        verify_html: "<html><body>verify</body></html>".to_string(),
        extras: Vec::new(),
    }
}

/// Mount the three Hub endpoints the happy-path flow hits. Each mock returns
/// the minimal JSON body hf-hub's deserializers require.
async fn mount_happy_path(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/repos/create"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "url": format!("https://huggingface.co/datasets/{TEST_REPO}"),
        })))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/api/datasets/.*/preupload/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "files": [
                {"path": "README.md", "uploadMode": "regular"},
                {"path": "croissant.json", "uploadMode": "regular"},
                {"path": "attestrum/manifest.parquet", "uploadMode": "regular"},
                {"path": "attestrum/merkle.root", "uploadMode": "regular"},
                {"path": "attestrum/bundle.sigstore.json", "uploadMode": "regular"},
                {"path": "attestrum/verify.html", "uploadMode": "regular"},
            ],
        })))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/api/datasets/.*/commit/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "commitUrl": format!("https://huggingface.co/datasets/{TEST_REPO}/commit/{TEST_COMMIT_OID}"),
            "commitOid": TEST_COMMIT_OID,
        })))
        .mount(server)
        .await;
}

#[test]
fn publish_happy_path_round_trips() {
    let rt = server_runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        mount_happy_path(&server).await;
        server
    });
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target = HuggingFaceTarget::new_with_endpoint(
        TEST_REPO.to_string(),
        TEST_BRANCH.to_string(),
        &server.uri(),
    )
    .expect("construct target with wiremock endpoint");

    let receipt: PublishReceipt = target.publish(&plan).expect("publish should succeed");

    assert_eq!(receipt.target, "huggingface");
    assert_eq!(
        receipt.dataset_url,
        format!("https://huggingface.co/datasets/{TEST_REPO}")
    );
    assert_eq!(
        receipt.verify_url,
        format!(
            "https://huggingface.co/datasets/{TEST_REPO}/blob/{TEST_BRANCH}/attestrum/verify.html"
        )
    );
    assert_eq!(receipt.commit_oid.as_deref(), Some(TEST_COMMIT_OID));
}

#[test]
fn publish_maps_401_to_auth() {
    let rt = server_runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/repos/create"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;
        server
    });
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target = HuggingFaceTarget::new_with_endpoint(
        TEST_REPO.to_string(),
        TEST_BRANCH.to_string(),
        &server.uri(),
    )
    .expect("construct target");

    let err = target.publish(&plan).expect_err("401 must error");
    assert!(
        matches!(err, AttestrumPublishError::Auth(_)),
        "expected Auth, got {err:?}"
    );
}

#[test]
fn publish_maps_429_to_quota() {
    let rt = server_runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        // hf-hub retries transient failures; 429 on every create_repo call
        // bottoms the retry loop out on the same status.
        Mock::given(method("POST"))
            .and(path("/api/repos/create"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;
        server
    });
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target = HuggingFaceTarget::new_with_endpoint(
        TEST_REPO.to_string(),
        TEST_BRANCH.to_string(),
        &server.uri(),
    )
    .expect("construct target");

    let err = target.publish(&plan).expect_err("429 must error");
    assert!(
        matches!(err, AttestrumPublishError::Quota(_)),
        "expected Quota, got {err:?}"
    );
}

#[test]
fn publish_treats_409_on_create_repo_as_idempotent() {
    let rt = server_runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        // 409 on create_repo + exist_ok=true is swallowed by hf-hub upstream —
        // the method returns a synthesized RepoUrl without raising. The
        // commit flow (preupload + commit) must still complete for publish()
        // to succeed.
        Mock::given(method("POST"))
            .and(path("/api/repos/create"))
            .respond_with(ResponseTemplate::new(409).set_body_string("already exists"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/api/datasets/.*/preupload/.*$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "files": [
                    {"path": "README.md", "uploadMode": "regular"},
                    {"path": "croissant.json", "uploadMode": "regular"},
                    {"path": "attestrum/manifest.parquet", "uploadMode": "regular"},
                    {"path": "attestrum/merkle.root", "uploadMode": "regular"},
                    {"path": "attestrum/bundle.sigstore.json", "uploadMode": "regular"},
                    {"path": "attestrum/verify.html", "uploadMode": "regular"},
                ],
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/api/datasets/.*/commit/.*$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "commitOid": TEST_COMMIT_OID,
            })))
            .mount(&server)
            .await;
        server
    });
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target = HuggingFaceTarget::new_with_endpoint(
        TEST_REPO.to_string(),
        TEST_BRANCH.to_string(),
        &server.uri(),
    )
    .expect("construct target");

    let receipt = target
        .publish(&plan)
        .expect("idempotent re-publish should succeed");
    assert_eq!(receipt.commit_oid.as_deref(), Some(TEST_COMMIT_OID));
}

#[test]
fn publish_maps_connection_refused_to_network() {
    // Bind a MockServer, capture its URI, then drop it BEFORE constructing the
    // target. The torn-down listener guarantees a connection-refused on the
    // first hf-hub call. (Picking a free port via OS rather than guessing one
    // sidesteps the determinism-matrix port-conflict risk per roadmap R3.)
    let rt = server_runtime();
    let uri = rt.block_on(async {
        let server = MockServer::start().await;
        server.uri()
    });
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target =
        HuggingFaceTarget::new_with_endpoint(TEST_REPO.to_string(), TEST_BRANCH.to_string(), &uri)
            .expect("construct target with torn-down endpoint");

    let err = target
        .publish(&plan)
        .expect_err("connection refused must error");
    assert!(
        matches!(err, AttestrumPublishError::Network(_)),
        "expected Network, got {err:?}"
    );
}
