//! S5-D3 E3 — wiremock-backed integration tests for `HuggingFaceTarget::publish()`.
//! S5-D3 E6 — fixture-plan refactor to the new `*_plan` field shape +
//!            `publish_commits_seven_ops_with_rendered_content` test that
//!            captures the create_commit NDJSON body and asserts the ops' paths
//!            and base64-decoded content against the render fns called
//!            separately (cyclonedx.json added 2026-05-30).
//!
//! Each test spins its own `wiremock::MockServer` on a dedicated tokio
//! runtime (clean isolation, sub-50ms startup) and points
//! `HuggingFaceTarget::new_with_endpoint()` at the mock's `uri()` instead of
//! `https://huggingface.co`. The six tests cover:
//!
//! 1. happy path — 7-op commit round-trips, receipt fields are correct.
//! 2. 401 on `/api/repos/create` → `AttestrumPublishError::Auth`.
//! 3. 429 on `/api/repos/create` → `AttestrumPublishError::Quota`.
//! 4. 409 on `/api/repos/create` with `exist_ok=true` → idempotent (commit completes).
//! 5. connection refused (mock server torn down before publish) → `Network`.
//! 6. (E6) commit body contains exactly the 7 expected paths in order, and the
//!    four `add_bytes` payloads decode to byte-equal `attestrum-emit` outputs.
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
//!
//! ## Body-capture pattern (E6)
//!
//! Test 6 captures the commit endpoint's NDJSON body via an
//! `Arc<Mutex<Vec<u8>>>` that's cloned into a `Respond` closure (the blanket
//! `Fn(&Request) -> ResponseTemplate: Respond` impl in wiremock 0.6). The
//! closure writes the body during request handling on the side runtime's
//! worker thread; the test thread reads it AFTER `publish()` returns. No
//! cross-runtime contention — the mutex is a single ordered handoff.

use std::path::Path;
use std::sync::{Arc, Mutex};

use attestrum_publish::{
    AttestrumPublishError, CroissantPlan, CycloneDxPlan, DatasetCardPlan, HuggingFaceTarget,
    ManifestStats, PublishPlan, PublishReceipt, PublishTarget, VerifyHtmlPlan,
};
use base64::Engine;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

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
///
/// E6: the three rendered-string fields (`croissant`, `readme`, `verify_html`)
/// from E3 are replaced with the three `*_plan` fields that drive
/// `attestrum-emit::render_*` at publish-time.
fn write_fixture_plan(dir: &Path) -> PublishPlan {
    std::fs::write(dir.join("manifest.parquet"), b"PAR1\x00\x00fake parquet")
        .expect("write manifest fixture");
    std::fs::write(dir.join("merkle.root"), [0xAB_u8; 32]).expect("write merkle.root fixture");
    std::fs::write(dir.join("bundle.sigstore.json"), b"{\"fake\":\"bundle\"}")
        .expect("write bundle fixture");
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
        cyclonedx_plan: CycloneDxPlan {
            dataset_name: TEST_REPO.to_string(),
            version: "1.0.0".to_string(),
            source_date_epoch: 1_700_000_000,
            manifest_sha256_hex: "a".repeat(64),
            merkle_root_blake3_hex: "b".repeat(64),
            manifest_stats: stats,
            license: Some("Apache-2.0".to_string()),
            publisher: None,
            classification: None,
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
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
            verify_url: format!(
                "https://huggingface.co/datasets/{TEST_REPO}/blob/{TEST_BRANCH}/attestrum/verify.html"
            ),
            attribution: None,
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
                {"path": "cyclonedx.json", "uploadMode": "regular"},
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
                {"path": "cyclonedx.json", "uploadMode": "regular"},
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
    // Target `127.0.0.1:1`. Port 1 is in the privileged range (< 1024) so
    // no unprivileged process — including another test's `MockServer` —
    // can bind it on Linux or macOS. `connect()` returns ECONNREFUSED
    // immediately.
    //
    // The E3-era version of this test bound a wiremock `MockServer`, captured
    // its URI, then dropped the server before issuing the request. That
    // assumed the OS would not re-allocate the freed port to a parallel
    // test's `MockServer` between drop and publish() — which is exactly the
    // race that landed in CI at S5-D3 E6 (a parallel test's happy-path mocks
    // served the supposedly-torn-down endpoint and publish() succeeded with
    // the wrong receipt). The privileged-port form is race-free.
    let uri = "http://127.0.0.1:1";
    let dir = TempDir::new().expect("tempdir");
    let plan = write_fixture_plan(dir.path());
    let target =
        HuggingFaceTarget::new_with_endpoint(TEST_REPO.to_string(), TEST_BRANCH.to_string(), uri)
            .expect("construct target with unreachable endpoint");

    let err = target
        .publish(&plan)
        .expect_err("connection refused must error");
    assert!(
        matches!(err, AttestrumPublishError::Network(_)),
        "expected Network, got {err:?}"
    );
}

/// E6 (+ cyclonedx-mlbom-shape): the create_commit endpoint receives exactly
/// the 7 expected file operations in order, and the four `add_bytes` payloads
/// decode to the byte-equal outputs of `attestrum_emit::render_*` called
/// separately. This is the contract that proves publish() actually consumes
/// attestrum-emit at runtime (rather than e.g. silently re-using a stale
/// fixture string).
#[test]
fn publish_commits_seven_ops_with_rendered_content() {
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_for_closure = Arc::clone(&captured);

    let rt = server_runtime();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/repos/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": format!("https://huggingface.co/datasets/{TEST_REPO}"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/api/datasets/.*/preupload/.*$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "files": [
                    {"path": "README.md", "uploadMode": "regular"},
                    {"path": "croissant.json", "uploadMode": "regular"},
                {"path": "cyclonedx.json", "uploadMode": "regular"},
                    {"path": "attestrum/manifest.parquet", "uploadMode": "regular"},
                    {"path": "attestrum/merkle.root", "uploadMode": "regular"},
                    {"path": "attestrum/bundle.sigstore.json", "uploadMode": "regular"},
                    {"path": "attestrum/verify.html", "uploadMode": "regular"},
                ],
            })))
            .mount(&server)
            .await;
        // Custom Respond closure capturing the NDJSON commit body before
        // returning the canned 200. wiremock 0.6 blanket-impls Respond for
        // any `Send + Sync + Fn(&Request) -> ResponseTemplate`; the closure
        // captures an `Arc<Mutex<Vec<u8>>>` which satisfies Send + Sync.
        Mock::given(method("POST"))
            .and(path_regex(r"^/api/datasets/.*/commit/.*$"))
            .respond_with(move |req: &Request| {
                *captured_for_closure.lock().unwrap() = req.body.clone();
                ResponseTemplate::new(200).set_body_json(json!({
                    "commitUrl": format!("https://huggingface.co/datasets/{TEST_REPO}/commit/{TEST_COMMIT_OID}"),
                    "commitOid": TEST_COMMIT_OID,
                }))
            })
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

    target.publish(&plan).expect("publish should succeed");

    let body = captured.lock().unwrap().clone();
    assert!(!body.is_empty(), "commit body must be captured");
    let body_str = std::str::from_utf8(&body).expect("commit body is UTF-8 NDJSON");

    // Parse NDJSON: one JSON value per line. Filter `"key": "file"` entries —
    // hf-hub also emits a leading `{"key":"header",...}` with the commit
    // message that we don't assert on here.
    let file_entries: Vec<Value> = body_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("NDJSON line parses"))
        .filter(|v| v.get("key").and_then(|k| k.as_str()) == Some("file"))
        .collect();

    assert_eq!(
        file_entries.len(),
        7,
        "expected 7 file operations, got {}: {body_str}",
        file_entries.len()
    );

    let expected_paths = [
        "README.md",
        "croissant.json",
        "cyclonedx.json",
        "attestrum/manifest.parquet",
        "attestrum/merkle.root",
        "attestrum/bundle.sigstore.json",
        "attestrum/verify.html",
    ];
    for (i, expected_path) in expected_paths.iter().enumerate() {
        let actual_path = file_entries[i]
            .pointer("/value/path")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("op {i} missing /value/path: {}", file_entries[i]));
        assert_eq!(
            actual_path, *expected_path,
            "op {i}: expected path {expected_path}, got {actual_path}"
        );
        let encoding = file_entries[i]
            .pointer("/value/encoding")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("op {i} missing /value/encoding"));
        assert_eq!(encoding, "base64", "op {i}: expected base64 encoding");
    }

    // The four add_bytes payloads MUST match what the emit fns produce on
    // the same plan. Re-call the render fns and compare base64-decoded
    // content. add_file payloads (parquet/merkle.root/bundle) are tested
    // for path + encoding above; their content is the raw file bytes which
    // the tempdir fixtures verify implicitly.
    let decoded_readme = decode_op_content(&file_entries[0]);
    let expected_readme =
        attestrum_emit::render_readme(&plan.dataset_card_plan).expect("render readme");
    assert_eq!(
        decoded_readme,
        expected_readme.into_bytes(),
        "README.md content must equal attestrum_emit::render_readme output"
    );

    let decoded_croissant = decode_op_content(&file_entries[1]);
    let expected_croissant =
        attestrum_emit::render_croissant(&plan.croissant_plan).expect("render croissant");
    assert_eq!(
        decoded_croissant,
        expected_croissant.into_bytes(),
        "croissant.json content must equal attestrum_emit::render_croissant output"
    );

    let decoded_cyclonedx = decode_op_content(&file_entries[2]);
    let expected_cyclonedx =
        attestrum_emit::render_cyclonedx(&plan.cyclonedx_plan).expect("render cyclonedx");
    assert_eq!(
        decoded_cyclonedx,
        expected_cyclonedx.into_bytes(),
        "cyclonedx.json content must equal attestrum_emit::render_cyclonedx output"
    );

    let decoded_verify_html = decode_op_content(&file_entries[6]);
    let expected_verify_html =
        attestrum_emit::render_verify_html_stub(&plan.verify_html_plan).expect("render verify");
    assert_eq!(
        decoded_verify_html,
        expected_verify_html.into_bytes(),
        "attestrum/verify.html content must equal attestrum_emit::render_verify_html_stub output"
    );
}

/// Base64-decode the `value.content` of a parsed NDJSON file-operation
/// entry. Panics with a helpful message if the structure is wrong.
fn decode_op_content(op: &Value) -> Vec<u8> {
    let b64 = op
        .pointer("/value/content")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("op missing /value/content: {op}"));
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64 decode")
}
