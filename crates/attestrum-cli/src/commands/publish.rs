//! `attestrum publish --dataset ORG/NAME --manifest PATH --bundle PATH …` —
//! Sprint 5 D3 E7. Wraps `attestrum_publish::HuggingFaceTarget::publish()`
//! end-to-end as a user-facing CLI subcommand.
//!
//! The CLI is the orchestrator: it reads the sealed manifest via
//! `attestrum_manifest::read_manifest` to derive [`ManifestStats`], reads
//! the Sigstore Bundle v0.3 JSON via `attestrum_attest::extract_identity`
//! to populate the verify.html stub's identity fields, constructs the
//! three plan-input structs (`CroissantPlan`, `DatasetCardPlan`,
//! `VerifyHtmlPlan`), packs them into a [`PublishPlan`], and calls
//! `target.publish(&plan)`.
//!
//! v0.1 scope (founder-locked at E7 plan time):
//!
//! - `--dataset <ORG/NAME>` validates the HF org/name shape (rejects empty,
//!   missing `/`). HF treats personal accounts and orgs identically; both
//!   forms are accepted (roadmap §OQ5).
//! - `--source-date-epoch` follows the `sign.rs` / `prove.rs` precedent:
//!   CLI flag > `SOURCE_DATE_EPOCH` env var > arg error. No wall-clock
//!   fallback per CLAUDE.md §7.
//! - Metadata flags (`--pretty-name`, `--license`, `--language`,
//!   `--task-category`, `--tag`, `--size-category`) are NOT present at
//!   v0.1 per Q1 of the plan-mode design questions. `pretty_name` is
//!   derived from the last segment of `--dataset` (with `-` and `_`
//!   replaced by spaces); `size_category` is derived from
//!   `manifest_stats.leaf_count` via the HF size-tag table; `license_spdx`
//!   defaults to a placeholder; `language`, `task_categories`, `tags` are
//!   empty vecs. Richer metadata deferred to v0.2.
//! - `--target {huggingface|github-release|static}` defaults to
//!   `huggingface`. The other two values reach the CLI surface but their
//!   `publish()` returns `AttestrumPublishError::NotImplemented(_)` per
//!   the v0.2-deferral SD3 lock from D3 planning.
//! - Output is human key-value lines to stdout (no `--output json` flag
//!   at v0.1; mirrors prove.rs / sign.rs).
//! - HF auth is implicit via the `hf-hub` token chain (`HF_TOKEN` env →
//!   `HF_TOKEN_PATH` file → `$HF_HOME/token`); no `--hf-token` flag at
//!   v0.1. The D3 refactor debt closed at E2 — `attestrum-publish` and
//!   `attestrum-prove` both delegate to hf-hub.
//!
//! Error → exit-code mapping uses the existing `lifecycle::ExitCode`
//! values (no new codes; mirrors the `AttestrumPublishError` 10-variant
//! lock from D3 E1):
//!
//! | `AttestrumPublishError` variant | `ExitCode`     | Numeric |
//! |---------------------------------|----------------|---------|
//! | `Network(_)`                    | `NetworkError` | 5       |
//! | `Auth(_)`                       | `IdentityError`| 4       |
//! | `RepoExists(_)`                 | `RuntimeError` | 1       |
//! | `RepoMissing(_)`                | `RuntimeError` | 1       |
//! | `Quota(_)`                      | `RuntimeError` | 1       |
//! | `BundleMissing(_)`              | `RuntimeError` | 1       |
//! | `ReadmeRender(_)`               | `RuntimeError` | 1       |
//! | `CroissantInvalid(_)`           | `RuntimeError` | 1       |
//! | `VerifyHtmlBuild(_)`            | `RuntimeError` | 1       |
//! | `NotImplemented(_)`             | `RuntimeError` | 1       |
//! | `Io(_)`                         | `RuntimeError` | 1       |
//! | arg-parse failure               | `ArgsError`    | 2       |
//! | success                         | `Ok`           | 0       |

use std::path::PathBuf;

use attestrum_attest::extract_identity;
use attestrum_manifest::read_manifest;
use attestrum_publish::{
    AttestrumPublishError, CroissantPlan, DatasetCardPlan, GitHubReleaseTarget, HuggingFaceTarget,
    ManifestStats, PublishPlan, PublishReceipt, PublishTarget, StaticBundleTarget, VerifyHtmlPlan,
};

use crate::lifecycle::ExitCode;

// ============================================================================
// Repo-relative paths committed by HuggingFaceTarget::publish()
// ============================================================================
//
// Mirrors the 6-file create_commit shape locked at
// `crates/attestrum-publish/src/lib.rs:224-245`. Centralised so the CLI
// hands the same strings into the three Plan structs as the publish
// surface uses on the wire.

const MANIFEST_PATH_IN_REPO: &str = "attestrum/manifest.parquet";
const BUNDLE_PATH_IN_REPO: &str = "attestrum/bundle.sigstore.json";
const MERKLE_ROOT_PATH_IN_REPO: &str = "attestrum/merkle.root";
const VERIFY_HTML_PATH_IN_REPO: &str = "attestrum/verify.html";

// Number of canonical files HuggingFaceTarget::publish() always commits
// (README.md, croissant.json, attestrum/manifest.parquet,
// attestrum/merkle.root, attestrum/bundle.sigstore.json,
// attestrum/verify.html). PublishReceipt doesn't surface this count, so
// the CLI computes `CANONICAL_FILES_COMMITTED + plan.extras.len()`.
const CANONICAL_FILES_COMMITTED: usize = 6;

// ============================================================================
// Args + entry point
// ============================================================================

#[derive(Debug)]
pub struct Args {
    /// `--dataset <ORG/NAME>`. Required. The HF Hub dataset repo. Personal
    /// accounts and organisations accepted identically.
    pub dataset: String,

    /// `--manifest <PATH>`. Required. Path to the sealed `manifest.parquet`
    /// (the output of `attestrum build`).
    pub manifest: PathBuf,

    /// `--bundle <PATH>`. Required. Path to the Sigstore Bundle v0.3 JSON
    /// (the output of `attestrum sign`). The CLI extracts the leaf cert's
    /// identity pair from this bundle for the verify.html stub.
    pub bundle: PathBuf,

    /// `--source-date-epoch <TS>`. Required: CLI flag > `SOURCE_DATE_EPOCH`
    /// env var > arg error. Feeds the Croissant emitter's `dateCreated`
    /// field. No wall-clock fallback per CLAUDE.md §7.
    pub source_date_epoch: Option<i64>,

    /// `--target {huggingface|github-release|static}`. Defaults to
    /// `huggingface`. The other two reach the CLI surface but their
    /// `publish()` returns `NotImplemented(_)` → exit 1 per SD3.
    pub target: TargetKind,

    /// `--revision <REV>`. The HF branch to commit against. Defaults to
    /// `main`.
    pub revision: String,

    /// `--workspace <DIR>`. Override the workspace dir used to locate the
    /// default Merkle-root path. None → `./.attestrum/`.
    pub workspace: Option<PathBuf>,

    /// `--merkle-root <PATH>`. Override the default Merkle-root file path.
    /// None → `<workspace>/.attestrum/manifests/merkle.root` (matches where
    /// `attestrum build` writes the file alongside `manifest.parquet`).
    pub merkle_root: Option<PathBuf>,

    /// `--token-file <PATH>`. Optional path to a file holding the HF Hub
    /// token. Reserved at v0.1 — the underlying `hf-hub` crate resolves
    /// its token chain (`HF_TOKEN` env → `HF_TOKEN_PATH` file →
    /// `$HF_HOME/token`); pointing `HF_TOKEN_PATH` at this value gives
    /// the documented behavior. Surfaced now to lock the CLI shape; the
    /// internal pass-through wires up alongside the v0.2 metadata-flag
    /// pass.
    pub token_file: Option<PathBuf>,

    /// `--no-sign-commits`. Skip GPG-signing git commits on the Hub.
    /// Reserved at v0.1 (hf-hub doesn't expose a per-commit GPG toggle
    /// directly; the flag locks the CLI shape).
    pub no_sign_commits: bool,

    /// `--out-dir <DIR>`. Only consulted by `--target static`. Reserved
    /// at v0.1; passing it under `--target huggingface` is ignored.
    pub out_dir: Option<PathBuf>,
}

/// The three publish-target values the CLI accepts. Mirrors
/// `attestrum_publish`'s three impls of [`PublishTarget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Huggingface,
    GithubRelease,
    Static,
}

impl TargetKind {
    /// Parse the `--target` flag value. Anything else → `ArgsError`.
    pub fn parse(arg: &str) -> Result<Self, String> {
        match arg {
            "huggingface" => Ok(TargetKind::Huggingface),
            "github-release" => Ok(TargetKind::GithubRelease),
            "static" => Ok(TargetKind::Static),
            other => Err(format!(
                "--target {other:?} not recognised; expected one of \
                 'huggingface', 'github-release', 'static'"
            )),
        }
    }
}

pub fn run(args: Args) -> u8 {
    // 1. Validate args.
    if let Err(msg) = validate_dataset(&args.dataset) {
        eprintln!("attestrum publish: {msg}");
        return ExitCode::ArgsError.as_u8();
    }
    if !args.manifest.is_file() {
        eprintln!(
            "attestrum publish: --manifest {:?} does not exist or is not a file",
            args.manifest
        );
        return ExitCode::ArgsError.as_u8();
    }
    if !args.bundle.is_file() {
        eprintln!(
            "attestrum publish: --bundle {:?} does not exist or is not a file",
            args.bundle
        );
        return ExitCode::ArgsError.as_u8();
    }

    // 2. Resolve source_date_epoch.
    let source_date_epoch = match resolve_source_date_epoch(&args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("attestrum publish: {msg}");
            return ExitCode::ArgsError.as_u8();
        }
    };

    // 3. Read manifest → derive ManifestStats.
    let manifest_stats = match read_manifest_stats(&args.manifest) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("attestrum publish: {msg}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    // 4. Read bundle → extract identity.
    let identity = match read_bundle_identity(&args.bundle) {
        Ok(i) => i,
        Err(msg) => {
            eprintln!("attestrum publish: {msg}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    // 5. Derive metadata (Q1 → A): defaults from inputs.
    let pretty_name = derive_pretty_name(&args.dataset);
    let size_category = derive_size_category(manifest_stats.leaf_count);

    // 6. Resolve the Merkle-root path. CLI override > <workspace>/manifests/merkle.root.
    let merkle_root_path = resolve_merkle_root_path(&args);

    let verify_url = verify_url_for(args.target, &args.dataset, &args.revision);

    let croissant_plan = CroissantPlan {
        dataset_name: args.dataset.clone(),
        manifest_path_in_repo: MANIFEST_PATH_IN_REPO.to_string(),
        bundle_path_in_repo: BUNDLE_PATH_IN_REPO.to_string(),
        merkle_root_path_in_repo: MERKLE_ROOT_PATH_IN_REPO.to_string(),
        manifest_stats,
        source_date_epoch,
        // license_spdx omitted at v0.1 per Q1 → A. Croissant emitter
        // skips the field entirely when None.
        license_spdx: None,
    };

    let dataset_card_plan = DatasetCardPlan {
        pretty_name,
        // license_spdx is a non-Option field on DatasetCardPlan, so we
        // must pass something. Apache-2.0 is the workspace's own dual-
        // license SPDX and matches the Croissant `license_spdx: None`
        // omission shape (caller signalled "don't know" / "no rich
        // metadata at v0.1"). v0.2 will surface a --license flag.
        license_spdx: "Apache-2.0".to_string(),
        language: Vec::new(),
        task_categories: Vec::new(),
        size_category,
        tags: Vec::new(),
        dataset_name: args.dataset.clone(),
        manifest_stats,
        verify_url,
    };

    let verify_html_plan = VerifyHtmlPlan {
        dataset_name: args.dataset.clone(),
        certificate_identity: identity.san,
        certificate_oidc_issuer: identity.oidc_issuer,
        bundle_path_in_repo: BUNDLE_PATH_IN_REPO.to_string(),
        manifest_path_in_repo: MANIFEST_PATH_IN_REPO.to_string(),
        // Same derived stats the Croissant + dataset-card plans use; the
        // verify page renders them as a human-readable corpus summary.
        // `ManifestStats` is `Copy`.
        manifest_stats,
    };

    let plan = PublishPlan {
        manifest_path: args.manifest.clone(),
        bundle_path: args.bundle.clone(),
        merkle_root_path,
        croissant_plan,
        dataset_card_plan,
        verify_html_plan,
        extras: Vec::new(),
    };

    // 7. Construct the target and call publish().
    let receipt = match dispatch_publish(&args, &plan) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("attestrum publish: {err}");
            return map_error_to_exit_code(&err).as_u8();
        }
    };

    print_summary(&receipt, &plan);
    ExitCode::Ok.as_u8()
}

// ============================================================================
// Validation + parsing helpers
// ============================================================================

/// Validate the `--dataset ORG/NAME` shape. Mirrors
/// `attestrum_publish::HuggingFaceTarget::new()`'s validation so the
/// CLI fails at parse-time rather than at target-construction time.
fn validate_dataset(arg: &str) -> Result<(), String> {
    if arg.is_empty() {
        return Err("--dataset is empty; expected ORG/NAME".to_string());
    }
    if !arg.contains('/') {
        return Err(format!("--dataset {arg:?} missing '/'; expected ORG/NAME"));
    }
    Ok(())
}

/// Resolve `--source-date-epoch` per the sign.rs / prove.rs precedent:
/// CLI flag wins; fall back to the `SOURCE_DATE_EPOCH` env var; error
/// if neither is set.
fn resolve_source_date_epoch(args: &Args) -> Result<i64, String> {
    if let Some(s) = args.source_date_epoch {
        return Ok(s);
    }
    if let Ok(s) = std::env::var("SOURCE_DATE_EPOCH") {
        return s
            .parse::<i64>()
            .map_err(|e| format!("SOURCE_DATE_EPOCH env var is not a valid integer: {s:?} ({e})"));
    }
    Err("required: pass --source-date-epoch <SECS> or set SOURCE_DATE_EPOCH env var".to_string())
}

/// Read the sealed manifest and compute the [`ManifestStats`] the two
/// emit plans embed. Bubble I/O / schema errors as human messages —
/// `read_manifest` already produces a clear context-laden error.
fn read_manifest_stats(path: &std::path::Path) -> Result<ManifestStats, String> {
    let entries = read_manifest(path).map_err(|e| format!("read manifest {path:?}: {e}"))?;
    let leaf_count = entries.len() as u64;
    let total_bytes = entries.iter().map(|e| e.size_bytes).sum();
    Ok(ManifestStats {
        leaf_count,
        total_bytes,
    })
}

/// Read the Sigstore Bundle v0.3 JSON and extract the leaf cert's
/// identity pair. Used to populate `VerifyHtmlPlan.{certificate_identity,
/// certificate_oidc_issuer}`.
fn read_bundle_identity(
    path: &std::path::Path,
) -> Result<attestrum_attest::ExtractedIdentity, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read bundle {path:?}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse bundle JSON {path:?}: {e}"))?;
    extract_identity(&value).map_err(|e| format!("extract identity from {path:?}: {e}"))
}

/// Resolve the Merkle-root path. CLI `--merkle-root` override wins; the
/// default points at `<workspace>/.attestrum/manifests/merkle.root` (the
/// path `attestrum build` writes alongside `manifest.parquet`).
fn resolve_merkle_root_path(args: &Args) -> PathBuf {
    if let Some(p) = args.merkle_root.clone() {
        return p;
    }
    let workspace = args.workspace.clone().unwrap_or_else(|| PathBuf::from("."));
    workspace
        .join(".attestrum")
        .join("manifests")
        .join("merkle.root")
}

/// Derive the dataset card `pretty_name` from `--dataset ORG/NAME`. The
/// last segment after `/` is taken and `-` / `_` are replaced with
/// spaces. Honors the roadmap §OQ3 default-derivation pattern.
fn derive_pretty_name(dataset: &str) -> String {
    let last_segment = dataset.rsplit('/').next().unwrap_or(dataset);
    last_segment.replace(['-', '_'], " ")
}

/// Build the `verify_url` embedded in the rendered README (the dataset
/// card's `attestrum.verify_url` field + provenance prose).
///
/// For the HF target it's the absolute Hub blob URL where the page will
/// live. For the **static** target the bundle directory is self-contained
/// and re-hostable (Zenodo, GitHub Pages, S3, …), so the page's absolute
/// URL is unknowable at publish time — we emit the **relative** repo path
/// `attestrum/verify.html`, which resolves correctly wherever the directory
/// lands (and when the README is rendered locally in `out_dir`). The
/// `PublishReceipt` separately carries an absolute `file://` URL for the
/// human running the CLI to open immediately. The github-release target
/// (a v0.2 deferral) reuses the HF-style absolute URL as a placeholder.
fn verify_url_for(target: TargetKind, dataset: &str, revision: &str) -> String {
    match target {
        TargetKind::Static => VERIFY_HTML_PATH_IN_REPO.to_string(),
        TargetKind::Huggingface | TargetKind::GithubRelease => format!(
            "https://huggingface.co/datasets/{dataset}/blob/{revision}/{VERIFY_HTML_PATH_IN_REPO}"
        ),
    }
}

/// Derive the HF `size_categories` tag from the manifest leaf count.
/// The 9 buckets match Hugging Face's documented dataset size tags.
fn derive_size_category(leaf_count: u64) -> String {
    match leaf_count {
        n if n < 1_000 => "n<1K",
        n if n < 10_000 => "1K<n<10K",
        n if n < 100_000 => "10K<n<100K",
        n if n < 1_000_000 => "100K<n<1M",
        n if n < 10_000_000 => "1M<n<10M",
        n if n < 100_000_000 => "10M<n<100M",
        n if n < 1_000_000_000 => "100M<n<1B",
        n if n < 10_000_000_000 => "1B<n<10B",
        _ => "n>10B",
    }
    .to_string()
}

// ============================================================================
// Target dispatch
// ============================================================================

/// Construct the requested `PublishTarget` impl and call `publish()`. The
/// two v0.2-deferred targets construct fine but their `publish()` returns
/// `NotImplemented(_)` — mapped to exit 1 like the other runtime errors.
fn dispatch_publish(
    args: &Args,
    plan: &PublishPlan,
) -> Result<PublishReceipt, AttestrumPublishError> {
    match args.target {
        TargetKind::Huggingface => {
            let target = HuggingFaceTarget::new(args.dataset.clone(), args.revision.clone())?;
            target.publish(plan)
        }
        TargetKind::GithubRelease => {
            // The github-release target's identity is a `repo` + `tag`
            // pair. v0.2-deferral — the actual values are inconsequential
            // because `publish()` returns NotImplemented unconditionally.
            // We use the dataset string + the revision so any future log
            // line surfaces sensible context.
            let target = GitHubReleaseTarget {
                repo: args.dataset.clone(),
                tag: args.revision.clone(),
            };
            target.publish(plan)
        }
        TargetKind::Static => {
            let out_dir = args
                .out_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from(".attestrum-static"));
            let target = StaticBundleTarget { out_dir };
            target.publish(plan)
        }
    }
}

// ============================================================================
// Error mapping + output
// ============================================================================

fn map_error_to_exit_code(err: &AttestrumPublishError) -> ExitCode {
    use AttestrumPublishError::*;
    match err {
        Network(_) => ExitCode::NetworkError,
        Auth(_) => ExitCode::IdentityError,
        RepoExists(_) | RepoMissing(_) | Quota(_) | BundleMissing(_) | ReadmeRender(_)
        | CroissantInvalid(_) | VerifyHtmlBuild(_) | NotImplemented(_) | Io(_) => {
            ExitCode::RuntimeError
        }
    }
}

fn print_summary(receipt: &PublishReceipt, plan: &PublishPlan) {
    println!("target:          {}", receipt.target);
    println!("dataset_url:     {}", receipt.dataset_url);
    println!("verify_url:      {}", receipt.verify_url);
    if let Some(oid) = &receipt.commit_oid {
        println!("commit_oid:      {oid}");
    }
    println!(
        "files_committed: {}",
        CANONICAL_FILES_COMMITTED + plan.extras.len()
    );
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with(dataset: &str, source_date_epoch: Option<i64>) -> Args {
        Args {
            dataset: dataset.to_string(),
            manifest: PathBuf::from("/tmp/manifest.parquet"),
            bundle: PathBuf::from("/tmp/bundle.sigstore.json"),
            source_date_epoch,
            target: TargetKind::Huggingface,
            revision: "main".to_string(),
            workspace: None,
            merkle_root: None,
            token_file: None,
            no_sign_commits: false,
            out_dir: None,
        }
    }

    #[test]
    fn validate_dataset_accepts_org_slash_name() {
        validate_dataset("my-org/my-dataset").expect("ORG/NAME shape is valid");
    }

    #[test]
    fn validate_dataset_rejects_empty() {
        validate_dataset("").expect_err("empty must error");
    }

    #[test]
    fn validate_dataset_rejects_missing_slash() {
        validate_dataset("just-a-name").expect_err("missing '/' must error");
    }

    #[test]
    fn derive_pretty_name_replaces_dashes_and_underscores() {
        assert_eq!(
            derive_pretty_name("my-org/my-cool_dataset"),
            "my cool dataset"
        );
    }

    #[test]
    fn derive_pretty_name_handles_missing_slash() {
        // validate_dataset rejects missing-slash before this is called in
        // production, but the helper must still be total. The whole string
        // becomes the last segment.
        assert_eq!(derive_pretty_name("solo-dataset"), "solo dataset");
    }

    #[test]
    fn derive_size_category_buckets_boundaries() {
        // Table-test the 9 HF buckets at zero, one-below, and the boundary
        // value of each band.
        assert_eq!(derive_size_category(0), "n<1K");
        assert_eq!(derive_size_category(999), "n<1K");
        assert_eq!(derive_size_category(1_000), "1K<n<10K");
        assert_eq!(derive_size_category(9_999), "1K<n<10K");
        assert_eq!(derive_size_category(10_000), "10K<n<100K");
        assert_eq!(derive_size_category(100_000), "100K<n<1M");
        assert_eq!(derive_size_category(1_000_000), "1M<n<10M");
        assert_eq!(derive_size_category(10_000_000), "10M<n<100M");
        assert_eq!(derive_size_category(100_000_000), "100M<n<1B");
        assert_eq!(derive_size_category(1_000_000_000), "1B<n<10B");
        assert_eq!(derive_size_category(10_000_000_000), "n>10B");
    }

    #[test]
    fn resolve_source_date_epoch_cli_flag_wins() {
        // CLI flag takes precedence even if the env var is also set. We
        // skip touching the env var to keep the test parallel-safe.
        let args = args_with("my-org/x", Some(1_700_000_000));
        let s = resolve_source_date_epoch(&args).expect("cli flag present");
        assert_eq!(s, 1_700_000_000);
    }

    #[test]
    fn resolve_source_date_epoch_neither_set_errors() {
        // Both CLI flag and env var absent. We assert the error message
        // mentions both sources so callers know what to set. Note: the
        // env var is process-global; if a parallel test sets it the
        // assertion may fail spuriously. Other tests in this module
        // deliberately do NOT touch SOURCE_DATE_EPOCH for that reason.
        let args = args_with("my-org/x", None);
        let env_was_set = std::env::var("SOURCE_DATE_EPOCH").is_ok();
        if env_was_set {
            // Skip: another test or the calling environment has set the
            // var. The CLI-flag-wins test already exercises the happy
            // path; the neither-set assertion can't be reliably made.
            return;
        }
        let err = resolve_source_date_epoch(&args).expect_err("neither set must error");
        assert!(
            err.contains("--source-date-epoch") && err.contains("SOURCE_DATE_EPOCH"),
            "err message must point at both sources, got: {err}"
        );
    }

    #[test]
    fn target_kind_parses_three_values() {
        assert_eq!(
            TargetKind::parse("huggingface").expect("ok"),
            TargetKind::Huggingface
        );
        assert_eq!(
            TargetKind::parse("github-release").expect("ok"),
            TargetKind::GithubRelease
        );
        assert_eq!(TargetKind::parse("static").expect("ok"), TargetKind::Static);
        TargetKind::parse("bogus").expect_err("unknown must error");
    }

    #[test]
    fn resolve_merkle_root_path_defaults_to_workspace_manifests() {
        let mut args = args_with("my-org/x", Some(1_700_000_000));
        args.workspace = Some(PathBuf::from("/some/work"));
        assert_eq!(
            resolve_merkle_root_path(&args),
            PathBuf::from("/some/work/.attestrum/manifests/merkle.root")
        );
    }

    #[test]
    fn resolve_merkle_root_path_cli_override_wins() {
        let mut args = args_with("my-org/x", Some(1_700_000_000));
        args.workspace = Some(PathBuf::from("/some/work"));
        args.merkle_root = Some(PathBuf::from("/explicit/root.bin"));
        assert_eq!(
            resolve_merkle_root_path(&args),
            PathBuf::from("/explicit/root.bin")
        );
    }

    #[test]
    fn verify_url_for_static_is_relative_else_absolute() {
        // Static target: relative repo path, re-hostable anywhere.
        assert_eq!(
            verify_url_for(TargetKind::Static, "my-org/x", "main"),
            "attestrum/verify.html"
        );
        // HF + github-release: absolute Hub blob URL.
        assert_eq!(
            verify_url_for(TargetKind::Huggingface, "my-org/x", "main"),
            "https://huggingface.co/datasets/my-org/x/blob/main/attestrum/verify.html"
        );
        assert_eq!(
            verify_url_for(TargetKind::GithubRelease, "my-org/x", "dev"),
            "https://huggingface.co/datasets/my-org/x/blob/dev/attestrum/verify.html"
        );
    }

    #[test]
    fn error_mapping_covers_all_eleven_variants() {
        // Locks the 11-variant `AttestrumPublishError` lock at the CLI
        // boundary. If a new variant lands without explicit CLI mapping,
        // the compiler will refuse to compile `map_error_to_exit_code`.
        use AttestrumPublishError::*;
        assert_eq!(
            map_error_to_exit_code(&Network("x".to_string())).as_u8(),
            ExitCode::NetworkError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&Auth("x".to_string())).as_u8(),
            ExitCode::IdentityError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&RepoExists("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&RepoMissing("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&Quota("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&BundleMissing("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&ReadmeRender("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&CroissantInvalid("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&VerifyHtmlBuild("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&NotImplemented("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&Io("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
    }
}
