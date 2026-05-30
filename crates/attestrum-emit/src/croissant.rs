//! Croissant 1.0 JSON-LD emitter.
//!
//! Spec: <https://docs.mlcommons.org/croissant/docs/croissant-spec.html>.
//!
//! Output is byte-deterministic: sorted-key JSON via
//! [`attestrum_attest::deterministic_json`] (which sorts nested-object keys
//! recursively, including the inlined `@context` dict), ISO-8601 `dateCreated`
//! / `datePublished` derived from [`CroissantPlan::source_date_epoch`] via
//! `jiff::Timestamp::from_second` (no wall-clock), repo-relative file paths.
//!
//! **`@context` is the full standard Croissant v1.0 context dict** (36 keys),
//! inlined, plus the `attestrum` extension key. The public `mlcroissant`
//! validator hard-requires `@context` to be a dict (a URL-ref array fails its
//! `get_context()` before any content check) and warns for every standard key
//! missing from it; extra keys (our `attestrum`) are not flagged. The 36 keys
//! match `mlcroissant`'s `make_context()` output for `conformsTo: .../1.0`
//! exactly — regenerate from the validator source if the pinned Croissant
//! version is bumped (decision `croissant-context-conformance`, 2026-05-30).
//! The `tests/croissant.rs` key-set test (37 keys = 36 standard + attestrum)
//! guards against drift.
//!
//! The Attestrum extension lives under the custom `attestrum:` namespace
//! prefix mapped to `https://attestrum.com/croissant/v0.1/`. "Attestrum"
//! appears only here, never in standard Croissant structure (CLAUDE.md §12).
//! The four extension URIs are:
//!
//! - `attestrum:predicate`    → `https://attestrum.com/attestation/training-corpus/v0.3` (§4 protected)
//! - `attestrum:manifest`     → `CroissantPlan::manifest_path_in_repo`
//! - `attestrum:merkleRoot`   → `CroissantPlan::merkle_root_path_in_repo`
//! - `attestrum:bundle`       → `CroissantPlan::bundle_path_in_repo`

use crate::{AttestrumEmitError, CroissantPlan};
use serde_json::json;

const ATTESTRUM_NAMESPACE_URL: &str = "https://attestrum.com/croissant/v0.1/";
const ATTESTRUM_PREDICATE_URI: &str = "https://attestrum.com/attestation/training-corpus/v0.3";

/// The Croissant spec version this emitter pins. Drives `dct:conformsTo` and
/// the inlined `@context` keyset. Bumping this requires regenerating the
/// `standard_context()` keys from `mlcroissant`'s `make_context()`.
const CROISSANT_CONFORMS_TO: &str = "http://mlcommons.org/croissant/1.0";

/// Build the Croissant 1.0 `@context` value: the full standard v1.0 dict
/// (matching `mlcroissant`'s `make_context()` for `conformsTo: .../1.0`) plus
/// the `attestrum` extension prefix. Inlined as a dict — `mlcroissant` requires
/// a dict and never dereferences a context URL.
fn standard_context() -> serde_json::Value {
    json!({
        "@language": "en",
        "@vocab": "https://schema.org/",
        "citeAs": "cr:citeAs",
        "column": "cr:column",
        "conformsTo": "dct:conformsTo",
        "cr": "http://mlcommons.org/croissant/",
        "data": { "@id": "cr:data", "@type": "@json" },
        "dataType": { "@id": "cr:dataType", "@type": "@vocab" },
        "dct": "http://purl.org/dc/terms/",
        "equivalentProperty": "cr:equivalentProperty",
        "examples": { "@id": "cr:examples", "@type": "@json" },
        "extract": "cr:extract",
        "field": "cr:field",
        "fileObject": "cr:fileObject",
        "fileProperty": "cr:fileProperty",
        "fileSet": "cr:fileSet",
        "format": "cr:format",
        "includes": "cr:includes",
        "isLiveDataset": "cr:isLiveDataset",
        "jsonPath": "cr:jsonPath",
        "key": "cr:key",
        "md5": "cr:md5",
        "parentField": "cr:parentField",
        "path": "cr:path",
        "rai": "http://mlcommons.org/croissant/RAI/",
        "recordSet": "cr:recordSet",
        "references": "cr:references",
        "regex": "cr:regex",
        "repeated": "cr:repeated",
        "replace": "cr:replace",
        "samplingRate": "cr:samplingRate",
        "sc": "https://schema.org/",
        "separator": "cr:separator",
        "source": "cr:source",
        "subField": "cr:subField",
        "transform": "cr:transform",
        // Attestrum vendor extension — the only attestrum-namespaced key.
        "attestrum": ATTESTRUM_NAMESPACE_URL,
    })
}

/// Render a Croissant 1.0 JSON-LD descriptor as a canonical-form JSON string.
///
/// The emitted file validates against the public `mlcroissant` validator with
/// zero errors. The four recommended fields (`license`, `version`,
/// `datePublished`, `citeAs`) each warn when absent; the CLI supplies
/// `license`/`version`/`datePublished` so a default publish carries at most one
/// warning (`citeAs`, publisher-only data) and validates zero/zero once a
/// citation is supplied. See module docs for the determinism contract, the
/// inlined-`@context` rationale, and the four Attestrum extension URIs.
pub fn render(plan: &CroissantPlan) -> Result<String, AttestrumEmitError> {
    let date = jiff::Timestamp::from_second(plan.source_date_epoch)
        .map_err(|e| {
            AttestrumEmitError::Croissant(format!(
                "source_date_epoch {} out of range: {e}",
                plan.source_date_epoch
            ))
        })?
        .to_string();

    let size = format!(
        "{} bytes across {} documents",
        plan.manifest_stats.total_bytes, plan.manifest_stats.leaf_count
    );

    let mut document = json!({
        "@context": standard_context(),
        "@type": "Dataset",
        "conformsTo": CROISSANT_CONFORMS_TO,
        "name": plan.dataset_name,
        "dateCreated": date,
        "datePublished": date,
        "isLiveDataset": false,
        "recordSet": [],
        "size": size,
        "attestrum:provenance": {
            "attestrum:predicate": ATTESTRUM_PREDICATE_URI,
            "attestrum:manifest": plan.manifest_path_in_repo,
            "attestrum:merkleRoot": plan.merkle_root_path_in_repo,
            "attestrum:bundle": plan.bundle_path_in_repo,
        },
    });

    // Recommended fields — emit only when supplied; never synthesize. The CLI
    // applies the honest defaults (license `"unknown"`, version `"1.0.0"`).
    if let Some(license) = &plan.license_spdx {
        document["license"] = json!(license);
    }
    if let Some(version) = &plan.version {
        document["version"] = json!(version);
    }
    if let Some(cite_as) = &plan.cite_as {
        document["citeAs"] = json!(cite_as);
    }

    attestrum_attest::deterministic_json(&document)
        .map_err(|e| AttestrumEmitError::Croissant(format!("serialize: {e}")))
}
