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
//! - The three Croissant-recommended metadata flags `--license`,
//!   `--version`, and `--cite-as` are present (decision
//!   `croissant-context-conformance`, 2026-05-30) so the emitted
//!   `croissant.json` can reach zero mlcroissant warnings. `--license`
//!   defaults to the honest token `"unknown"` (threaded into BOTH the
//!   Croissant descriptor and the README so they agree); `--version`
//!   defaults to `"1.0.0"`; `--cite-as` is omitted when absent (never
//!   synthesized). `--pretty-name` is OPTIONAL: when supplied it sets the
//!   dataset card's display title, otherwise it is derived from the last
//!   segment of `--dataset` (with `-` and `_` replaced by spaces). The
//!   remaining metadata flags (`--language`, `--task-category`, `--tag`,
//!   `--size-category`) are NOT present: `size_category` is derived from
//!   `manifest_stats.leaf_count` via the HF size-tag table; `language`,
//!   `task_categories`, `tags` are empty vecs. Richer metadata deferred to v0.2.
//! - `--target {huggingface|github-release|static}` defaults to
//!   `huggingface`. `static` writes the artifact set to a local `--out-dir`
//!   (Stage A1). `github-release` remains a v0.2 deferral — its `publish()`
//!   returns `AttestrumPublishError::NotImplemented(_)` → exit 1.
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
//! | `CycloneDxInvalid(_)`           | `RuntimeError` | 1       |
//! | `VerifyHtmlBuild(_)`            | `RuntimeError` | 1       |
//! | `NotImplemented(_)`             | `RuntimeError` | 1       |
//! | `Io(_)`                         | `RuntimeError` | 1       |
//! | arg-parse failure               | `ArgsError`    | 2       |
//! | success                         | `Ok`           | 0       |

use std::path::PathBuf;

use attestrum_attest::{extract_identity, statement_from_bundle, TrainingCorpusPredicate};
use attestrum_manifest::read_manifest;
use attestrum_publish::{
    AttestrumPublishError, CroissantPlan, CycloneDxPlan, DatasetCardPlan, GitHubReleaseTarget,
    HuggingFaceTarget, ManifestStats, PublishPlan, PublishReceipt, PublishTarget,
    StaticBundleTarget, VerifyHtmlPlan,
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
// (README.md, croissant.json, cyclonedx.json, attestrum/manifest.parquet,
// attestrum/merkle.root, attestrum/bundle.sigstore.json,
// attestrum/verify.html). PublishReceipt doesn't surface this count, so
// the CLI computes `CANONICAL_FILES_COMMITTED + plan.extras.len()`.
const CANONICAL_FILES_COMMITTED: usize = 7;

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

    /// `--license <SPDX>`. Optional corpus license, threaded into both the
    /// Croissant `license` field and the dataset-card README. `None` →
    /// the honest token `"unknown"` (accepted by mlcroissant and the HF
    /// Hub) rather than a fabricated license.
    pub license: Option<String>,

    /// `--version <SEMVER>`. Optional dataset version for the Croissant
    /// `version` field. `None` → `"1.0.0"` (first sealed release).
    pub version: Option<String>,

    /// `--cite-as <TEXT>`. Optional citation for the Croissant `citeAs`
    /// field. `None` → the field is omitted (never synthesized).
    pub cite_as: Option<String>,

    /// `--publisher <ORG>`. Optional corpus-publisher organisation name for
    /// the CycloneDX ML-BOM. When supplied it populates the dataset
    /// `supplier` and `componentData.governance.owners`; `None` omits both
    /// (honest omission). For public demos this is the Attestrum GitHub
    /// Actions workflow identity — never an individual (CLAUDE-LOCAL §A9).
    pub publisher: Option<String>,

    /// `--classification <LABEL>`. Optional data-classification / sensitivity
    /// label for the CycloneDX `componentData.classification` (e.g.
    /// `public`). `None` omits it (never fabricated).
    pub classification: Option<String>,

    /// `--target {huggingface|github-release|static}`. Defaults to
    /// `huggingface`. `static` writes to a local `--out-dir` (Stage A1);
    /// `github-release` is a v0.2 deferral whose `publish()` returns
    /// `NotImplemented(_)` → exit 1.
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

    /// `--out-dir <DIR>`. Only consulted by `--target static` (the local
    /// output directory; defaults to `.attestrum-static`). Ignored by the
    /// other targets.
    pub out_dir: Option<PathBuf>,

    /// `--attribution-file <PATH>`. Optional path to a markdown file whose
    /// contents are rendered verbatim as the dataset card's
    /// `## Source & attribution` section (e.g. CC-BY-SA-3.0 source/credit/
    /// ShareAlike for a Wikipedia-derived corpus). `None` → no such section.
    /// The CLI authors no attribution text; the publisher supplies it.
    pub attribution_file: Option<PathBuf>,

    /// `--pretty-name <TITLE>`. Optional human-friendly dataset display title
    /// for the card heading + provenance prose. `None` → derived from the
    /// `--dataset` slug (`derive_pretty_name`: org dropped, `-`/`_` → spaces).
    pub pretty_name: Option<String>,
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

    // 4b. Read the signed subject SHA-256 + Merkle root from the bundle for the
    //     CycloneDX ML-BOM (honesty invariant: SHA-256 in `hashes`, BLAKE3
    //     Merkle root in a namespaced property).
    let (manifest_sha256, merkle_root_blake3) = match read_bundle_corpus_digests(&args.bundle) {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("attestrum publish: {msg}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    // 5. Derive metadata (Q1 → A): defaults from inputs.
    // CLI override > derived-from-slug. The explicit title lets a publisher set a
    // proper display name (e.g. "WikiText-103 (Attestrum-sealed)") instead of the
    // slug-derived "wikitext 103 sealed".
    let pretty_name = args
        .pretty_name
        .clone()
        .unwrap_or_else(|| derive_pretty_name(&args.dataset));
    let size_category = derive_size_category(manifest_stats.leaf_count);

    // 6. Resolve the Merkle-root path. CLI override > <workspace>/manifests/merkle.root.
    let merkle_root_path = resolve_merkle_root_path(&args);

    let verify_url = verify_url_for(args.target, &args.dataset, &args.revision);

    // Resolve the corpus license ONCE, shared by the Croissant descriptor and
    // the dataset-card README so the two artifacts never disagree. When the
    // publisher gives no --license, record the honest token "unknown" (a value
    // both mlcroissant and the HF Hub accept) rather than asserting a license
    // the publisher didn't declare. Default the version to "1.0.0" (first
    // sealed release; overridable via --version); citeAs is omitted unless
    // supplied (never synthesized). Decision `croissant-context-conformance`.
    let license = args
        .license
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    if args.license.is_none() {
        eprintln!(
            "attestrum publish: no --license supplied; recording license \"unknown\" \
             (pass --license <SPDX> to declare one)"
        );
    }
    let version = args.version.clone().unwrap_or_else(|| "1.0.0".to_string());

    let croissant_plan = CroissantPlan {
        dataset_name: args.dataset.clone(),
        manifest_path_in_repo: MANIFEST_PATH_IN_REPO.to_string(),
        bundle_path_in_repo: BUNDLE_PATH_IN_REPO.to_string(),
        merkle_root_path_in_repo: MERKLE_ROOT_PATH_IN_REPO.to_string(),
        manifest_stats,
        source_date_epoch,
        license_spdx: Some(license.clone()),
        version: Some(version.clone()),
        // Emit citeAs only when the publisher supplies it; omission is the
        // honest default (mlcroissant emits one benign recommended-field
        // warning), never a synthesized citation.
        cite_as: args.cite_as.clone(),
    };

    // The CycloneDX ML-BOM reuses the same resolved license/version as the
    // Croissant descriptor so the two sidecars agree. The two signed digests
    // come from the bundle (4b); --publisher / --classification drive the
    // honest-omission identity/governance fields.
    let cyclonedx_plan = CycloneDxPlan {
        dataset_name: args.dataset.clone(),
        version,
        source_date_epoch,
        manifest_sha256_hex: manifest_sha256,
        merkle_root_blake3_hex: merkle_root_blake3,
        manifest_stats,
        license: Some(license.clone()),
        publisher: args.publisher.clone(),
        classification: args.classification.clone(),
        manifest_path_in_repo: MANIFEST_PATH_IN_REPO.to_string(),
        bundle_path_in_repo: BUNDLE_PATH_IN_REPO.to_string(),
    };

    // Optional attribution markdown, read verbatim from --attribution-file and
    // rendered as the card's `## Source & attribution` section (license-required
    // credit / source / modification disclosure / ShareAlike for, e.g., a
    // CC-BY-SA-3.0 Wikipedia-derived corpus). Absent → no section. The CLI
    // authors none of this text; the publisher supplies the file.
    let attribution = match &args.attribution_file {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("attestrum publish: --attribution-file {path:?}: {e}");
                return ExitCode::RuntimeError.as_u8();
            }
        },
        None => None,
    };

    let dataset_card_plan = DatasetCardPlan {
        pretty_name,
        // Same resolved license as the Croissant descriptor (real value or the
        // honest "unknown" token) — the two artifacts must agree.
        license_spdx: license,
        language: Vec::new(),
        task_categories: Vec::new(),
        size_category,
        tags: Vec::new(),
        dataset_name: args.dataset.clone(),
        manifest_stats,
        verify_url,
        attribution,
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
        cyclonedx_plan,
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

/// Read the manifest's signed SHA-256 subject digest and the BLAKE3 Merkle
/// root from the signed bundle's in-toto Statement, for the CycloneDX ML-BOM.
///
/// The SHA-256 is the value CycloneDX `hashes` carries — the Sigstore-signed
/// in-toto subject digest of `manifest.parquet`, which a third party can
/// recompute (`sha256sum manifest.parquet`) and match. The BLAKE3 Merkle root
/// (read from the signed training-corpus predicate) goes in a namespaced
/// `attestrum:` property, never in `hashes`. Both come from the one signed
/// payload so the ML-BOM binds to what was actually signed (decision
/// `cyclonedx-mlbom-shape`). Returns `(manifest_sha256_hex, merkle_root_blake3_hex)`.
fn read_bundle_corpus_digests(path: &std::path::Path) -> Result<(String, String), String> {
    let statement =
        statement_from_bundle(path).map_err(|e| format!("read statement from {path:?}: {e}"))?;
    let subject = statement
        .subject
        .first()
        .ok_or_else(|| format!("bundle {path:?} has no in-toto subject"))?;
    let manifest_sha256 = subject.digest.sha256.clone();
    let predicate: TrainingCorpusPredicate = serde_json::from_value(statement.predicate.clone())
        .map_err(|e| format!("parse training-corpus predicate from {path:?}: {e}"))?;
    Ok((manifest_sha256, predicate.merkle_root))
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

/// Construct the requested `PublishTarget` impl and call `publish()`.
/// `huggingface` and `static` are real impls; `github-release` constructs
/// fine but its `publish()` returns `NotImplemented(_)` — mapped to exit 1
/// like the other runtime errors.
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
        | CroissantInvalid(_) | CycloneDxInvalid(_) | VerifyHtmlBuild(_) | NotImplemented(_)
        | Io(_) => ExitCode::RuntimeError,
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
            license: None,
            version: None,
            cite_as: None,
            publisher: None,
            classification: None,
            target: TargetKind::Huggingface,
            revision: "main".to_string(),
            workspace: None,
            merkle_root: None,
            token_file: None,
            no_sign_commits: false,
            out_dir: None,
            attribution_file: None,
            pretty_name: None,
        }
    }

    #[test]
    fn pretty_name_override_beats_slug_derivation() {
        // With no --pretty-name, the title derives from the slug.
        assert_eq!(
            derive_pretty_name("Attestrum/wikitext-103-sealed"),
            "wikitext 103 sealed"
        );
        // The CLI override (args.pretty_name) takes precedence over the derivation;
        // this mirrors the run() resolution `args.pretty_name.unwrap_or_else(derive)`.
        let mut a = args_with("Attestrum/wikitext-103-sealed", Some(0));
        a.pretty_name = Some("WikiText-103 (Attestrum-sealed)".to_string());
        let resolved = a
            .pretty_name
            .clone()
            .unwrap_or_else(|| derive_pretty_name(&a.dataset));
        assert_eq!(resolved, "WikiText-103 (Attestrum-sealed)");
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
    fn error_mapping_covers_all_twelve_variants() {
        // Locks the 12-variant `AttestrumPublishError` lock at the CLI
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
            map_error_to_exit_code(&CycloneDxInvalid("x".to_string())).as_u8(),
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
