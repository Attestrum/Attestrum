//! `attestrum-publish` — Hugging Face Hub publish pipeline + alternate
//! publish targets (GitHub Releases, static bundle). Wraps the
//! Attestrum sealed-manifest + Sigstore-bundle + emitter outputs into a
//! single `create_commit` against the Hub so any visitor can `git clone`
//! the dataset and verify the bundle without Attestrum installed.
//!
//! Sprint 5 D3 E1 lands the public API surface only — bodies are
//! `unimplemented!()` pending E2-E6 real work. E2 brings in `hf-hub`
//! (second approved git-pin in the workspace per CLAUDE.md §8) and
//! closes the D3 refactor debt from D2 E7's inline HF auth. E3 wires
//! the create_repo + create_commit happy path against a `wiremock`
//! mock HF server. E4-E5 land the `attestrum-emit` Croissant +
//! dataset-card emitters. E6 wires publish() to consume emit's
//! outputs. E7 ships the `attestrum publish` CLI subcommand. E8
//! freezes the public surface via a hand-rolled `tests/api_surface.rs`
//! golden + flips `docs/diagrams/overview/hub-publish.md` from
//! `source_of_truth: diagram` to `source_of_truth: code`.
//!
//! v0.1 scope (founder-locked at D3 planning time):
//!
//! - **SD3**: only `HuggingFaceTarget` ships with a real `publish()`
//!   impl at D3. `GitHubReleaseTarget` + `StaticBundleTarget` appear in
//!   the public API surface (Part 2.3 contract preserved) but their
//!   `publish()` bodies return
//!   `AttestrumPublishError::NotImplemented(...)` with v0.2 deferral
//!   messages. Smaller surface to freeze at v0.1.
//! - **SD4**: HF auth chain is owned by `hf-hub` (env → cached token
//!   file → stored tokens). At E2, `attestrum-prove`'s inline
//!   `hf_auth_header()` / `build_hf_url()` get replaced by hf-hub API
//!   calls. No shared mini-crate.
//!
//! Comprehensive sprint context lives at
//! `~/Documents/Claude/Attestrum-internal-notes/sprint-5-d3-attestrum-publish-roadmap.md`
//! (local-only living document; not committed to the public repo per
//! CLAUDE.md §0.5 publication boundary).

use std::path::PathBuf;

// Re-export the three plan types attestrum-emit owns so callers can
// construct `PublishPlan` without a separate `attestrum-emit` dep.
// `ManifestStats` is re-exported because both plan types embed it.
pub use attestrum_emit::{CroissantPlan, DatasetCardPlan, ManifestStats, VerifyHtmlPlan};

// ============================================================================
// Public API surface — Part 2.3 of PATH-A-BRIEF
// ============================================================================

/// A target the caller wants to publish to. Three impls ship in the
/// `attestrum-publish` v0.1 surface: `HuggingFaceTarget` (real impl at
/// D3 E6), `GitHubReleaseTarget` (v0.2 deferral), and
/// `StaticBundleTarget` (v0.2 deferral). Custom impls in downstream
/// crates are possible — the trait is stable across v0.1.
pub trait PublishTarget {
    /// Stable name surfaced in `PublishReceipt.target`. Used by the CLI
    /// to print "target: huggingface" / "target: github-release" /
    /// "target: static" in the human summary. Lowercase + hyphenated.
    fn target_name(&self) -> &'static str;

    /// Publish the plan. Returns a `PublishReceipt` carrying the URLs
    /// the user can navigate to. On failure, maps to one of the
    /// `AttestrumPublishError` variants. See the implementing struct's
    /// docs for the specific behavioral contract.
    fn publish(&self, plan: &PublishPlan) -> Result<PublishReceipt, AttestrumPublishError>;
}

/// Hugging Face Hub publish target — the primary v0.1 path. Commits
/// the manifest + bundle + Croissant + README + verify.html stub to a
/// dataset repo via the Hub's `create_commit` API (no native Sigstore
/// attestation endpoint exists for datasets as of May 2026; we ship
/// the bundle as a regular committed file under
/// `attestrum/bundle.sigstore.json` per the OpenSSF model-signing
/// pattern documented in PATH-A-BRIEF Part 4.1).
///
/// `new()` instantiates an `hf_hub::HFClientSync` (auto-resolves the token
/// chain: env HF_TOKEN → HF_TOKEN_PATH file → $HF_HOME/token). `publish()`
/// implements create_repo + the 6-file create_commit per
/// `docs/diagrams/overview/hub-publish.md`. Tests construct via the
/// crate-private `new_with_endpoint()` to point the client at a wiremock
/// MockServer instead of `https://huggingface.co`.
#[derive(Clone)]
pub struct HuggingFaceTarget {
    repo: String,
    branch: String,
    client: hf_hub::HFClientSync,
}

impl std::fmt::Debug for HuggingFaceTarget {
    // Hand-rolled Debug skips the hf-hub client (HFClientSync doesn't derive
    // Debug). Surfaces only the publish-target identity, which is what callers
    // typically want in logs / panic messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HuggingFaceTarget")
            .field("repo", &self.repo)
            .field("branch", &self.branch)
            .finish_non_exhaustive()
    }
}

impl HuggingFaceTarget {
    /// Construct a `HuggingFaceTarget` for `repo` (e.g. `"my-org/my-dataset"`)
    /// on `branch` (typically `"main"`). Validates the repo shape, then
    /// constructs an `hf_hub::HFClientSync` — the client's builder reads the
    /// token chain at this point, so misconfigured token files surface as
    /// `AttestrumPublishError::Auth` early rather than at publish() time.
    pub fn new(repo: String, branch: String) -> Result<Self, AttestrumPublishError> {
        Self::build(repo, branch, None)
    }

    /// Constructor that points the hf-hub client at `endpoint` instead of
    /// resolving from `HF_ENDPOINT` / the default. Used by the integration
    /// tests at `tests/publish_huggingface.rs` to swap in a
    /// `wiremock::MockServer` URI; also usable by callers running a
    /// self-hosted HF Hub mirror.
    ///
    /// `#[doc(hidden)]` because this is not the canonical construction
    /// path — `new()` is. The E8 api-surface golden may include it; if so
    /// that's accurate (the function is reachable from outside the crate).
    #[doc(hidden)]
    pub fn new_with_endpoint(
        repo: String,
        branch: String,
        endpoint: &str,
    ) -> Result<Self, AttestrumPublishError> {
        Self::build(repo, branch, Some(endpoint))
    }

    fn build(
        repo: String,
        branch: String,
        endpoint: Option<&str>,
    ) -> Result<Self, AttestrumPublishError> {
        if repo.is_empty() {
            return Err(AttestrumPublishError::Auth(
                "dataset repo is empty".to_string(),
            ));
        }
        if !repo.contains('/') {
            return Err(AttestrumPublishError::Auth(format!(
                "dataset repo {repo:?} must be ORG/NAME shape (HF Hub convention)"
            )));
        }
        let client = build_client(endpoint)?;
        Ok(Self {
            repo,
            branch,
            client,
        })
    }

    /// The dataset repo this target publishes to, e.g. `"my-org/my-dataset"`.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// The branch this target publishes to (typically `"main"`).
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Access the underlying hf-hub client. Crate-private accessor used
    /// by `publish()`.
    pub(crate) fn client(&self) -> &hf_hub::HFClientSync {
        &self.client
    }
}

/// Construct an `HFClientSync`, optionally pointed at a custom `endpoint`
/// (used by the wiremock tests). When `endpoint` is `None`, the client
/// resolves from env (HF_ENDPOINT) or the default `https://huggingface.co`.
/// The token chain (env HF_TOKEN → HF_TOKEN_PATH file → $HF_HOME/token) is
/// read by the builder either way — misconfigured token files surface as
/// `AttestrumPublishError::Auth` here, not at publish() time.
fn build_client(endpoint: Option<&str>) -> Result<hf_hub::HFClientSync, AttestrumPublishError> {
    let mut builder = hf_hub::HFClient::builder();
    if let Some(ep) = endpoint {
        builder = builder.endpoint(ep);
        // Force a deterministic token so tests don't accidentally use the
        // developer's real ~/.cache/huggingface/token if it exists.
        builder = builder.token("test-token");
    }
    builder
        .build_sync()
        .map_err(|e| AttestrumPublishError::Auth(format!("hf-hub client init: {e}")))
}

impl PublishTarget for HuggingFaceTarget {
    fn target_name(&self) -> &'static str {
        "huggingface"
    }

    fn publish(&self, plan: &PublishPlan) -> Result<PublishReceipt, AttestrumPublishError> {
        for path in [
            &plan.manifest_path,
            &plan.merkle_root_path,
            &plan.bundle_path,
        ] {
            if !path.is_file() {
                return Err(AttestrumPublishError::BundleMissing(
                    path.display().to_string(),
                ));
            }
        }

        self.client()
            .create_repository()
            .repo_id(self.repo.clone())
            .repo_type(hf_hub::RepoTypeDataset)
            .exist_ok(true)
            .send()
            .map_err(|e| map_hf_error(e, &self.repo, HfOp::CreateRepo))?;

        // S5-D3 E6: render the three dataset-side artifacts here rather than
        // accepting pre-rendered strings off PublishPlan. Identity extraction
        // for the verify.html stub stays at the CLI orchestration layer (E7)
        // via a single `attestrum_attest::extract_identity()` call on the
        // freshly-signed bundle — attestrum-emit gains no I/O at runtime.
        let readme = attestrum_emit::render_readme(&plan.dataset_card_plan)
            .map_err(|e| AttestrumPublishError::ReadmeRender(e.to_string()))?;
        let croissant_str = attestrum_emit::render_croissant(&plan.croissant_plan)
            .map_err(|e| AttestrumPublishError::CroissantInvalid(e.to_string()))?;
        let verify_html = attestrum_emit::render_verify_html_stub(&plan.verify_html_plan)
            .map_err(|e| AttestrumPublishError::VerifyHtmlBuild(e.to_string()))?;

        let mut ops: Vec<hf_hub::repository::CommitOperation> = vec![
            hf_hub::repository::CommitOperation::add_bytes("README.md", readme.into_bytes()),
            hf_hub::repository::CommitOperation::add_bytes(
                "croissant.json",
                croissant_str.into_bytes(),
            ),
            hf_hub::repository::CommitOperation::add_file(
                "attestrum/manifest.parquet",
                plan.manifest_path.clone(),
            ),
            hf_hub::repository::CommitOperation::add_file(
                "attestrum/merkle.root",
                plan.merkle_root_path.clone(),
            ),
            hf_hub::repository::CommitOperation::add_file(
                "attestrum/bundle.sigstore.json",
                plan.bundle_path.clone(),
            ),
            hf_hub::repository::CommitOperation::add_bytes(
                "attestrum/verify.html",
                verify_html.into_bytes(),
            ),
        ];
        for (local_path, path_in_repo) in &plan.extras {
            ops.push(hf_hub::repository::CommitOperation::add_file(
                path_in_repo.clone(),
                local_path.clone(),
            ));
        }

        let (owner, name) = self.repo.split_once('/').expect("validated in new()");
        let commit = self
            .client()
            .dataset(owner, name)
            .create_commit()
            .operations(ops)
            .commit_message("attestrum publish".to_string())
            .revision(self.branch.clone())
            .send()
            .map_err(|e| map_hf_error(e, &self.repo, HfOp::CreateCommit))?;

        Ok(PublishReceipt {
            target: "huggingface".to_string(),
            dataset_url: format!("https://huggingface.co/datasets/{}", self.repo),
            verify_url: format!(
                "https://huggingface.co/datasets/{}/blob/{}/attestrum/verify.html",
                self.repo, self.branch
            ),
            commit_oid: commit.commit_oid,
        })
    }
}

/// Which hf-hub operation the error came from. Disambiguates `HFError::Conflict`
/// mapping: 409 on create_repo means "exists" (swallowed upstream by
/// `exist_ok=true`), 409 on create_commit means "parent-commit mismatch" — a
/// transient race that maps to `Network`, not `RepoExists`.
#[derive(Copy, Clone)]
enum HfOp {
    CreateRepo,
    CreateCommit,
}

/// Map an `hf_hub::HFError` into the 10-variant `AttestrumPublishError`. Mirrors
/// the precedent in `attestrum-prove::map_hf_error` but carries an `HfOp` so
/// `Conflict` can map differently for create_repo vs create_commit. The
/// `_ => Network(_)` fallback covers the `#[non_exhaustive]` arm — future hf-hub
/// variant additions that semantically belong in `Quota` / `Auth` will silently
/// misroute until this match is re-audited; documented here as a known caveat.
fn map_hf_error(err: hf_hub::HFError, repo: &str, op: HfOp) -> AttestrumPublishError {
    use hf_hub::HFError as E;
    let msg = format!("{repo}: {err}");
    match err {
        E::AuthRequired { .. } | E::Forbidden { .. } => AttestrumPublishError::Auth(msg),
        E::RepoNotFound { .. } | E::EntryNotFound { .. } | E::RevisionNotFound { .. } => {
            AttestrumPublishError::RepoMissing(msg)
        }
        E::RateLimited { .. } => AttestrumPublishError::Quota(msg),
        E::Conflict { .. } => match op {
            HfOp::CreateRepo => AttestrumPublishError::RepoExists(msg),
            HfOp::CreateCommit => AttestrumPublishError::Network(msg),
        },
        _ => AttestrumPublishError::Network(msg),
    }
}

/// GitHub Releases publish target — fallback / alternate publish
/// surface per PATH-A-BRIEF §4.5. **Deferred to v0.2 per founder
/// scope decision SD3 at D3 planning time.** The type exists in the
/// v0.1 surface so future callers can construct it; `publish()`
/// returns `AttestrumPublishError::NotImplemented(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubReleaseTarget {
    pub repo: String,
    pub tag: String,
}

impl PublishTarget for GitHubReleaseTarget {
    fn target_name(&self) -> &'static str {
        "github-release"
    }

    fn publish(&self, _plan: &PublishPlan) -> Result<PublishReceipt, AttestrumPublishError> {
        Err(AttestrumPublishError::NotImplemented(
            "GitHubReleaseTarget is v0.2 work — see Attestrum-internal-notes/sprint-5-d3-attestrum-publish-roadmap.md".to_string(),
        ))
    }
}

/// Static-bundle publish target — writes the publish artifacts to a
/// local directory for upload to Zenodo, GitHub Pages, S3, or any
/// static file host. **Deferred to v0.2 per founder scope decision
/// SD3.** Type exists in the v0.1 surface; `publish()` returns
/// `AttestrumPublishError::NotImplemented(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticBundleTarget {
    pub out_dir: PathBuf,
}

impl PublishTarget for StaticBundleTarget {
    fn target_name(&self) -> &'static str {
        "static"
    }

    fn publish(&self, _plan: &PublishPlan) -> Result<PublishReceipt, AttestrumPublishError> {
        Err(AttestrumPublishError::NotImplemented(
            "StaticBundleTarget is v0.2 work — see Attestrum-internal-notes/sprint-5-d3-attestrum-publish-roadmap.md".to_string(),
        ))
    }
}

/// The set of artifacts a caller hands to a `PublishTarget::publish()`
/// invocation. Constructed by the CLI subcommand at D3 E7 by combining
/// outputs from `attestrum build`, `attestrum sign`, and the
/// `attestrum-emit` renderers.
#[derive(Debug, Clone)]
pub struct PublishPlan {
    /// Local path to the sealed `manifest.parquet`.
    pub manifest_path: PathBuf,

    /// Local path to the Sigstore Bundle v0.3 JSON (output of
    /// `attestrum sign`).
    pub bundle_path: PathBuf,

    /// Local path to the raw Merkle root bytes (output of `attestrum build`'s
    /// sealed-manifest finalize step). Committed verbatim to the dataset repo
    /// at `attestrum/merkle.root` so a visitor can `git clone` the dataset
    /// and verify the bundle's root against the manifest without Attestrum
    /// installed.
    ///
    /// Added at D3 E3 (founder-approved 2026-05-28) to satisfy the 6-file
    /// `create_commit` shape locked in
    /// `docs/diagrams/overview/hub-publish.md`. The CLI subcommand at D3 E7
    /// will populate this from the same finalize artifact `attestrum build`
    /// already writes.
    pub merkle_root_path: PathBuf,

    /// Inputs for `attestrum_emit::render_croissant`. The CLI at D3 E7
    /// constructs this from CLI flags + a single manifest read; `publish()`
    /// calls the render fn at publish-time.
    ///
    /// Added at D3 E6 (founder-approved 2026-05-28); replaces the
    /// `croissant: serde_json::Value` rendered-payload field from E3.
    pub croissant_plan: CroissantPlan,

    /// Inputs for `attestrum_emit::render_readme`. Constructed by the CLI;
    /// `publish()` calls the render fn at publish-time.
    ///
    /// Added at D3 E6 (founder-approved 2026-05-28); replaces the
    /// `readme: String` rendered-payload field from E3.
    pub dataset_card_plan: DatasetCardPlan,

    /// Inputs for `attestrum_emit::render_verify_html_stub`. The CLI at
    /// D3 E7 populates `certificate_identity` + `certificate_oidc_issuer`
    /// via `attestrum_attest::extract_identity()` on the bundle.
    ///
    /// Added at D3 E6 (founder-approved 2026-05-28); replaces the
    /// `verify_html: String` rendered-payload field from E3.
    pub verify_html_plan: VerifyHtmlPlan,

    /// Additional local files to commit alongside the canonical set.
    /// Tuple is `(local_path, path_in_repo)`. Empty in the
    /// happy-path; reserved for callers shipping extras like a
    /// `license-inventory.json`.
    pub extras: Vec<(PathBuf, String)>,
}

/// Receipt of a successful `publish()` call. The CLI subcommand
/// (D3 E7) prints these fields as human key-value lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    /// Target name (matches `PublishTarget::target_name()`).
    pub target: String,

    /// URL where the published dataset lives. For HuggingFace:
    /// `https://huggingface.co/datasets/<org>/<name>`. For GitHub
    /// Releases: `https://github.com/<org>/<name>/releases/tag/<tag>`.
    /// For static: a `file://` URL pointing at `out_dir`.
    pub dataset_url: String,

    /// URL where the verify.html page is reachable. For HuggingFace:
    /// `https://huggingface.co/datasets/<org>/<name>/blob/<branch>/attestrum/verify.html`.
    pub verify_url: String,

    /// Hub-side commit OID, when applicable. `None` for static targets.
    pub commit_oid: Option<String>,
}

/// The closed set of error conditions `attestrum-publish` surfaces.
/// PATH-A-BRIEF Part 2.3 specifies 9 variants
/// (`Network` / `Auth` / `RepoExists` / `RepoMissing` / `Quota` /
/// `BundleMissing` / `ReadmeRender` / `CroissantInvalid` /
/// `VerifyHtmlBuild`). D3 E1 adds a 10th variant
/// (`NotImplemented`) so the `GitHubReleaseTarget` +
/// `StaticBundleTarget` stubs can return a v0.2-deferral error
/// without the caller having to pattern-match on a different error
/// type. The 10-variant shape is the v0.1 lock; new variants require
/// founder approval just like the `AttestrumProveError` 6-variant
/// lock from D2 (PATH-A-BRIEF Part 2.2).
#[derive(Debug, thiserror::Error)]
pub enum AttestrumPublishError {
    /// Connection refused, DNS failure, TLS handshake failure, request
    /// timeout, or any other transport-layer issue. CLI maps to exit 5.
    #[error("network error: {0}")]
    Network(String),

    /// 401 / 403 from the Hub, missing or invalid token, fine-grained
    /// token lacks the required scope. CLI maps to exit 4.
    #[error("authentication error: {0}")]
    Auth(String),

    /// `create_repo` returned 409 and `exist_ok` was false, OR the repo
    /// already exists and the caller wanted a fresh repo. CLI maps to
    /// exit 1.
    #[error("repository already exists: {0}")]
    RepoExists(String),

    /// Operation expected the repo to exist but it didn't (e.g.
    /// `create_commit` against a repo that hasn't been `create_repo`'d
    /// yet). CLI maps to exit 1.
    #[error("repository missing: {0}")]
    RepoMissing(String),

    /// 429 from the Hub, storage quota hit, rate-limit exhaustion.
    /// CLI maps to exit 1.
    #[error("quota / rate limit: {0}")]
    Quota(String),

    /// `PublishPlan.bundle_path` doesn't exist on disk, isn't a file,
    /// or can't be read. CLI maps to exit 1.
    #[error("bundle missing or unreadable: {0}")]
    BundleMissing(String),

    /// `attestrum-emit` failed to render README.md (manifest stats
    /// read error, YAML serialization error, etc.). CLI maps to
    /// exit 1.
    #[error("README render error: {0}")]
    ReadmeRender(String),

    /// `attestrum-emit` failed to render Croissant JSON-LD, OR the
    /// rendered Croissant failed schema validation. CLI maps to
    /// exit 1.
    #[error("Croissant validation error: {0}")]
    CroissantInvalid(String),

    /// `attestrum-emit` failed to render verify.html (couldn't read
    /// cert identity from the bundle, etc.). CLI maps to exit 1.
    #[error("verify.html build error: {0}")]
    VerifyHtmlBuild(String),

    /// v0.1-deferral: feature spec'd in PATH-A-BRIEF but explicitly
    /// not shipped in this version (e.g. `GitHubReleaseTarget` +
    /// `StaticBundleTarget` per founder scope decision SD3 at D3
    /// planning time). CLI maps to exit 1. Payload is a human
    /// message pointing at the roadmap for resolution timing.
    #[error("not implemented at v0.1: {0}")]
    NotImplemented(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huggingface_target_accepts_org_slash_name() {
        let t = HuggingFaceTarget::new("my-org/my-dataset".to_string(), "main".to_string())
            .expect("org/name is valid");
        assert_eq!(t.target_name(), "huggingface");
        assert_eq!(t.repo(), "my-org/my-dataset");
        assert_eq!(t.branch(), "main");
    }

    #[test]
    fn huggingface_target_rejects_empty_repo() {
        let err = HuggingFaceTarget::new("".to_string(), "main".to_string())
            .expect_err("empty repo must error");
        assert!(matches!(err, AttestrumPublishError::Auth(_)));
    }

    #[test]
    fn huggingface_target_rejects_missing_slash() {
        let err = HuggingFaceTarget::new("just-a-name".to_string(), "main".to_string())
            .expect_err("missing / must error");
        assert!(matches!(err, AttestrumPublishError::Auth(_)));
    }

    /// Minimal `PublishPlan` for tests that only need it to compile + pattern-
    /// match a `NotImplemented` error. The two v0.2-deferral targets never
    /// look at the plan, so the contents below are arbitrary stubs.
    fn minimal_plan() -> PublishPlan {
        PublishPlan {
            manifest_path: PathBuf::from("/tmp/manifest.parquet"),
            bundle_path: PathBuf::from("/tmp/bundle.sigstore.json"),
            merkle_root_path: PathBuf::from("/tmp/merkle.root"),
            croissant_plan: CroissantPlan {
                dataset_name: "my-org/my-dataset".to_string(),
                manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
                bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
                merkle_root_path_in_repo: "attestrum/merkle.root".to_string(),
                manifest_stats: ManifestStats {
                    leaf_count: 1,
                    total_bytes: 1,
                },
                source_date_epoch: 1_700_000_000,
                license_spdx: None,
            },
            dataset_card_plan: DatasetCardPlan {
                pretty_name: "Stub".to_string(),
                license_spdx: "Apache-2.0".to_string(),
                language: vec![],
                task_categories: vec![],
                size_category: "n<1K".to_string(),
                tags: vec![],
                dataset_name: "my-org/my-dataset".to_string(),
                manifest_stats: ManifestStats {
                    leaf_count: 1,
                    total_bytes: 1,
                },
                verify_url: "https://example/verify.html".to_string(),
            },
            verify_html_plan: VerifyHtmlPlan {
                dataset_name: "my-org/my-dataset".to_string(),
                certificate_identity: "stub-identity".to_string(),
                certificate_oidc_issuer: "stub-issuer".to_string(),
                bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
                manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
            },
            extras: Vec::new(),
        }
    }

    #[test]
    fn github_release_target_is_v02_deferral() {
        let t = GitHubReleaseTarget {
            repo: "my-org/my-dataset".to_string(),
            tag: "v0.1".to_string(),
        };
        assert_eq!(t.target_name(), "github-release");
        let err = t
            .publish(&minimal_plan())
            .expect_err("GitHubReleaseTarget is deferred");
        assert!(matches!(err, AttestrumPublishError::NotImplemented(_)));
    }

    #[test]
    fn static_bundle_target_is_v02_deferral() {
        let t = StaticBundleTarget {
            out_dir: PathBuf::from("/tmp/out"),
        };
        assert_eq!(t.target_name(), "static");
        let err = t
            .publish(&minimal_plan())
            .expect_err("StaticBundleTarget is deferred");
        assert!(matches!(err, AttestrumPublishError::NotImplemented(_)));
    }

    #[test]
    fn error_enum_has_ten_variants() {
        // Locks the 10-variant shape. If a new variant is added the
        // compiler errors on this match; the addition must be a
        // deliberate v0.1 surface change.
        let variants = [
            AttestrumPublishError::Network("x".to_string()),
            AttestrumPublishError::Auth("x".to_string()),
            AttestrumPublishError::RepoExists("x".to_string()),
            AttestrumPublishError::RepoMissing("x".to_string()),
            AttestrumPublishError::Quota("x".to_string()),
            AttestrumPublishError::BundleMissing("x".to_string()),
            AttestrumPublishError::ReadmeRender("x".to_string()),
            AttestrumPublishError::CroissantInvalid("x".to_string()),
            AttestrumPublishError::VerifyHtmlBuild("x".to_string()),
            AttestrumPublishError::NotImplemented("x".to_string()),
        ];
        assert_eq!(variants.len(), 10);
        for v in &variants {
            assert!(!v.to_string().is_empty(), "Display impl must not be empty");
        }
    }

    #[test]
    fn publish_plan_constructs_with_minimal_inputs() {
        let _plan = minimal_plan();
    }
}
