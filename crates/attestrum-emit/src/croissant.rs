//! Croissant 1.0 JSON-LD emitter.
//!
//! Spec: <https://docs.mlcommons.org/croissant/docs/croissant-spec.html>.
//!
//! Output is byte-deterministic: sorted-key JSON via
//! [`attestrum_attest::deterministic_json`], ISO-8601 `dateCreated` derived
//! from [`CroissantPlan::source_date_epoch`] via `jiff::Timestamp::from_second`
//! (no wall-clock), repo-relative file paths.
//!
//! The Attestrum extension lives under a custom `attestrum:` namespace
//! prefix mapped to `https://attestrum.com/croissant/v0.1/` in the
//! `@context` array (S5-D3 E4 plan-mode 2026-05-28 founder decision —
//! idiomatic JSON-LD vendor-extension pattern; the brief's literal
//! `cr:attestrumProvenance` shape would have claimed to extend the Croissant
//! vocabulary itself). The four extension URIs are:
//!
//! - `attestrum:predicate`    → `https://attestrum.com/attestation/training-corpus/v0.3`
//! - `attestrum:manifest`     → `CroissantPlan::manifest_path_in_repo`
//! - `attestrum:merkleRoot`   → `CroissantPlan::merkle_root_path_in_repo`
//! - `attestrum:bundle`       → `CroissantPlan::bundle_path_in_repo`

use crate::{AttestrumEmitError, CroissantPlan};
use serde_json::json;

const CROISSANT_CONTEXT_URL: &str = "http://mlcommons.org/croissant/1.0/context.json";
const ATTESTRUM_NAMESPACE_URL: &str = "https://attestrum.com/croissant/v0.1/";
const ATTESTRUM_PREDICATE_URI: &str = "https://attestrum.com/attestation/training-corpus/v0.3";

/// Render a Croissant 1.0 JSON-LD descriptor as a canonical-form JSON
/// string. See module docs for the determinism contract and the four
/// Attestrum extension URIs.
pub fn render(plan: &CroissantPlan) -> Result<String, AttestrumEmitError> {
    let date_created = jiff::Timestamp::from_second(plan.source_date_epoch)
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
        "@context": [
            CROISSANT_CONTEXT_URL,
            { "attestrum": ATTESTRUM_NAMESPACE_URL }
        ],
        "@type": "sc:Dataset",
        "name": plan.dataset_name,
        "dateCreated": date_created,
        "cr:isLiveDataset": false,
        "cr:recordSet": [],
        "size": size,
        "attestrum:provenance": {
            "attestrum:predicate": ATTESTRUM_PREDICATE_URI,
            "attestrum:manifest": plan.manifest_path_in_repo,
            "attestrum:merkleRoot": plan.merkle_root_path_in_repo,
            "attestrum:bundle": plan.bundle_path_in_repo,
        },
    });

    if let Some(license) = &plan.license_spdx {
        document["license"] = json!(license);
    }

    attestrum_attest::deterministic_json(&document)
        .map_err(|e| AttestrumEmitError::Croissant(format!("serialize: {e}")))
}
