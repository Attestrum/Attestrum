//! `attestrum-emit` — render the dataset-side artifacts that
//! `attestrum-publish` commits to the Hub: Croissant JSON-LD (the
//! schema.org/Dataset descriptor + Attestrum provenance extension), the
//! dataset card README.md (YAML frontmatter + provenance prose), and
//! the verify.html stub (a static page pointing visitors at the CLI
//! verify command — the real in-browser verifier ships in v0.2).
//!
//! Sprint 5 D3 E1 lands the public API surface only — bodies are
//! `unimplemented!()`. E4 ships the Croissant emitter; E5 ships the
//! README emitter; E6 ships the verify.html stub renderer. E8 freezes
//! the surface via a hand-rolled `tests/api_surface.rs` golden.
//!
//! Determinism contract: every render function MUST be byte-identical
//! across the 4-target CI matrix. Sorted-key JSON output via serde
//! (`serde_json::ser::PrettyFormatter` with explicit key ordering),
//! sorted YAML keys, no wall-clock fields. The `source_date_epoch`
//! field on `CroissantPlan` is the only acceptable timestamp source.
//!
//! Comprehensive sprint context lives at
//! `~/Documents/Claude/Attestrum-internal-notes/sprint-5-d3-attestrum-publish-roadmap.md`
//! (local-only living document; not committed to the public repo per
//! CLAUDE.md §0.5 publication boundary).

pub mod croissant;

// ============================================================================
// Public API surface
// ============================================================================

/// Render a Croissant JSON-LD descriptor (`croissant.json`) for the
/// dataset. The output is a string of canonical-form JSON suitable for
/// commit-to-Hub. Body fills in at D3 E4.
///
/// The output includes the standard schema.org `@context` block,
/// `@type: sc:Dataset`, plus an Attestrum-extension
/// `cr:attestrumProvenance` field linking to
/// `attestrum/manifest.parquet`, `attestrum/merkle.root`, and
/// `attestrum/bundle.sigstore.json`. The Hub's auto-generated
/// `/croissant` endpoint will continue to serve its own Parquet-derived
/// JSON-LD; ours is the publisher-authored authoritative one.
pub fn render_croissant(plan: &CroissantPlan) -> Result<String, AttestrumEmitError> {
    croissant::render(plan)
}

/// Render the dataset card `README.md` for the dataset. YAML
/// frontmatter (per PATH-A-BRIEF Part 2.3 spec) + provenance prose
/// + verify URL. Body fills in at D3 E5.
pub fn render_readme(_plan: &DatasetCardPlan) -> Result<String, AttestrumEmitError> {
    unimplemented!("S5-D3 E5 lands the dataset card emitter")
}

/// Render the v0.1 verify.html stub. A single self-contained static
/// page (~2 KB) that displays the Sigstore identity policy and tells
/// the visitor what CLI command to run to verify the bundle. The real
/// in-browser verifier (WASM cosign-lite per PATH-A-BRIEF Part 2.3)
/// is deferred to v0.2 per founder scope decision SD2 at D3 planning
/// time. Body fills in at D3 E6.
pub fn render_verify_html_stub(_plan: &VerifyHtmlPlan) -> Result<String, AttestrumEmitError> {
    unimplemented!("S5-D3 E6 lands the verify.html stub renderer")
}

// ============================================================================
// Plan input types
// ============================================================================

/// Caller-supplied inputs for `render_croissant()`. All file-path
/// fields are repo-relative (e.g. `"attestrum/manifest.parquet"`, not
/// `"/home/user/.attestrum/manifests/manifest.parquet"`) — the Hub
/// commit operations use repo-relative paths.
#[derive(Debug, Clone)]
pub struct CroissantPlan {
    /// Dataset name as it appears in the Hub URL (e.g.
    /// `"my-org/my-dataset"`).
    pub dataset_name: String,

    /// Repo-relative path to the sealed manifest file. Conventionally
    /// `"attestrum/manifest.parquet"`.
    pub manifest_path_in_repo: String,

    /// Repo-relative path to the Sigstore bundle. Conventionally
    /// `"attestrum/bundle.sigstore.json"`.
    pub bundle_path_in_repo: String,

    /// Repo-relative path to the Merkle root file. Conventionally
    /// `"attestrum/merkle.root"`.
    pub merkle_root_path_in_repo: String,

    /// Derived manifest statistics for the Croissant content fields.
    pub manifest_stats: ManifestStats,

    /// Reproducible-Builds timestamp (epoch seconds). Drives the
    /// Croissant `dateCreated` field deterministically. Matches the
    /// `--source-date-epoch` value used during `attestrum sign`.
    pub source_date_epoch: i64,

    /// Single SPDX license identifier (e.g. `"Apache-2.0"`, `"MIT"`,
    /// `"CC-BY-4.0"`) for the dataset's `license` field, OR the literal
    /// `"mixed"` when the corpus carries multiple licenses, OR `None` when
    /// the caller has no license info to assert. When `None` the emitter
    /// omits the `license` field entirely rather than synthesizing a value.
    ///
    /// Added at D3 E4 (founder-approved 2026-05-28). Parallel to
    /// `DatasetCardPlan.license_spdx` from E1; the CLI at D3 E7 will
    /// populate both from the same source (CLI flag, corpus.toml field, or
    /// manifest license-inventory if present).
    pub license_spdx: Option<String>,
}

/// Caller-supplied inputs for `render_readme()`. The YAML frontmatter
/// fields here mirror the Hub's documented dataset-card schema (per
/// PATH-A-BRIEF Part 2.3). Extension fields (`attestrum:` block) are
/// hardcoded by `render_readme` from the plan's manifest/bundle paths.
#[derive(Debug, Clone)]
pub struct DatasetCardPlan {
    /// Human-friendly dataset display name (e.g.
    /// `"My Dataset (v0.1)"`). When not provided by the user via CLI
    /// flag, the CLI derives it from `--dataset org/name` per OQ3 in
    /// the roadmap.
    pub pretty_name: String,

    /// Single SPDX license identifier (e.g. `"Apache-2.0"`,
    /// `"MIT"`, `"CC-BY-4.0"`), OR the literal string `"mixed"` when
    /// the corpus carries multiple licenses (in which case
    /// `render_readme` also writes a `license_details` field
    /// pointing at `attestrum/license-inventory.json`).
    pub license_spdx: String,

    /// ISO 639 language codes (e.g. `vec!["en"]`,
    /// `vec!["en", "es", "fr"]`). Order is preserved in the YAML output.
    pub language: Vec<String>,

    /// HF task category tags (e.g. `vec!["text-generation"]`,
    /// `vec!["image-classification"]`). Empty vec is allowed.
    pub task_categories: Vec<String>,

    /// HF size-category string (e.g. `"n<1K"`, `"1K<n<10K"`,
    /// `"1B<n<10B"`). Derived from `manifest_stats.leaf_count` by the
    /// CLI before construction.
    pub size_category: String,

    /// Hub tags appearing in the YAML `tags:` field. The
    /// `attestrum-provenance`, `sigstore-signed`, `croissant` tags
    /// are always appended by `render_readme`; this vec is for
    /// additional caller-supplied tags.
    pub tags: Vec<String>,

    /// Dataset name (as appears in Hub URL, `"my-org/my-dataset"`).
    pub dataset_name: String,

    /// Derived manifest statistics. Used in the provenance section
    /// prose ("This dataset contains N documents totaling X bytes
    /// across the following modalities: ...").
    pub manifest_stats: ManifestStats,

    /// URL to the verify.html page in the Hub repo. Used in both the
    /// YAML `attestrum.verify_url` field and the provenance section
    /// prose. Typically
    /// `"https://huggingface.co/datasets/<org>/<name>/blob/<branch>/attestrum/verify.html"`.
    pub verify_url: String,
}

/// Caller-supplied inputs for `render_verify_html_stub()`. The output
/// is a static HTML page with no JS or WASM; it shows the cert
/// identity policy + the CLI command the visitor should run.
#[derive(Debug, Clone)]
pub struct VerifyHtmlPlan {
    /// Dataset name (used in `<title>` + heading).
    pub dataset_name: String,

    /// Subject Alternative Name from the bundle's leaf certificate.
    /// Extracted by `attestrum-attest`'s verify flow; the CLI passes
    /// it through. Example:
    /// `"https://github.com/my-org/my-dataset/.github/workflows/build.yml@refs/heads/main"`.
    pub certificate_identity: String,

    /// Fulcio OIDC-issuer extension from the bundle's leaf cert.
    /// Example: `"https://token.actions.githubusercontent.com"`.
    pub certificate_oidc_issuer: String,

    /// Repo-relative path to the bundle file. The stub HTML embeds
    /// this in the suggested CLI command.
    pub bundle_path_in_repo: String,

    /// Repo-relative path to the manifest file. Same usage.
    pub manifest_path_in_repo: String,
}

/// Derived statistics from the sealed manifest. Populated by the CLI
/// at D3 E7 by reading the manifest via `attestrum-manifest` and
/// passed into `CroissantPlan` + `DatasetCardPlan`.
///
/// Additional fields land at E4 / E5 as the emitters need them
/// (modality histogram, schema_version, signal-coverage summary).
/// E1 ships the minimal pair the emitters definitely consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestStats {
    /// Number of leaves (documents) in the manifest.
    pub leaf_count: u64,

    /// Total byte count across all manifest entries' `bytes` column.
    pub total_bytes: u64,
}

/// The closed set of error conditions `attestrum-emit` surfaces. Five
/// variants at v0.1; the shape is locked at E1. New variants require
/// founder approval per the same convention as `AttestrumProveError`
/// (PATH-A-BRIEF Part 2.2) and `AttestrumPublishError` (Part 2.3).
#[derive(Debug, thiserror::Error)]
pub enum AttestrumEmitError {
    /// Failed to read or interpret the sealed manifest. Surfaced when
    /// `ManifestStats` derivation hits an I/O error or schema-version
    /// mismatch.
    #[error("manifest read error: {0}")]
    Manifest(String),

    /// Failed to read or interpret the Sigstore bundle. Surfaced when
    /// `render_verify_html_stub` can't extract the cert identity /
    /// OIDC issuer from the bundle's leaf cert.
    #[error("bundle read error: {0}")]
    Bundle(String),

    /// Failed to render Croissant JSON-LD. Surfaced when serde
    /// serialization fails or the assembled document doesn't pass
    /// internal validation (schema.org `@context` block missing,
    /// required Attestrum extension fields absent, etc.).
    #[error("Croissant JSON-LD error: {0}")]
    Croissant(String),

    /// Failed to render the dataset card README. Surfaced when the
    /// YAML frontmatter can't be serialized or when required fields
    /// on `DatasetCardPlan` are empty (e.g. `pretty_name`).
    #[error("README render error: {0}")]
    Readme(String),

    /// Failed to render the verify.html stub. Surfaced when the cert
    /// identity / OIDC issuer values on `VerifyHtmlPlan` are empty
    /// or contain HTML-unsafe characters that can't be escaped.
    #[error("verify.html render error: {0}")]
    VerifyHtml(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_stats_constructs() {
        let s = ManifestStats {
            leaf_count: 100,
            total_bytes: 1_048_576,
        };
        assert_eq!(s.leaf_count, 100);
        assert_eq!(s.total_bytes, 1_048_576);
    }

    #[test]
    fn croissant_plan_constructs() {
        let _p = CroissantPlan {
            dataset_name: "my-org/my-dataset".to_string(),
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
            merkle_root_path_in_repo: "attestrum/merkle.root".to_string(),
            manifest_stats: ManifestStats {
                leaf_count: 1,
                total_bytes: 1,
            },
            source_date_epoch: 1700000000,
            license_spdx: Some("Apache-2.0".to_string()),
        };
    }

    #[test]
    fn dataset_card_plan_constructs() {
        let _p = DatasetCardPlan {
            pretty_name: "My Dataset".to_string(),
            license_spdx: "Apache-2.0".to_string(),
            language: vec!["en".to_string()],
            task_categories: vec!["text-generation".to_string()],
            size_category: "n<1K".to_string(),
            tags: vec!["example".to_string()],
            dataset_name: "my-org/my-dataset".to_string(),
            manifest_stats: ManifestStats {
                leaf_count: 1,
                total_bytes: 1,
            },
            verify_url:
                "https://huggingface.co/datasets/my-org/my-dataset/blob/main/attestrum/verify.html"
                    .to_string(),
        };
    }

    #[test]
    fn verify_html_plan_constructs() {
        let _p = VerifyHtmlPlan {
            dataset_name: "my-org/my-dataset".to_string(),
            certificate_identity:
                "https://github.com/my-org/my-dataset/.github/workflows/build.yml@refs/heads/main"
                    .to_string(),
            certificate_oidc_issuer: "https://token.actions.githubusercontent.com".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
        };
    }

    #[test]
    fn error_enum_has_five_variants() {
        // Locks the 5-variant shape. New variants require founder
        // approval per the v0.1 surface contract.
        let variants = [
            AttestrumEmitError::Manifest("x".to_string()),
            AttestrumEmitError::Bundle("x".to_string()),
            AttestrumEmitError::Croissant("x".to_string()),
            AttestrumEmitError::Readme("x".to_string()),
            AttestrumEmitError::VerifyHtml("x".to_string()),
        ];
        assert_eq!(variants.len(), 5);
        for v in &variants {
            assert!(!v.to_string().is_empty(), "Display impl must not be empty");
        }
    }
}
