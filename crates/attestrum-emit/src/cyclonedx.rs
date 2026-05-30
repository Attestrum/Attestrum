//! CycloneDX 1.6 ML-BOM emitter (`cyclonedx.json`).
//!
//! Spec: CycloneDX 1.6, ECMA-424 (<https://cyclonedx.org/docs/1.6/json/>).
//!
//! The emitted file represents the sealed corpus as a single `data` component
//! carrying one `componentData{type:"dataset"}`. It validates against the
//! public CycloneDX validator (`sbom-utility`) with zero errors / zero
//! warnings (CLAUDE.md §12 vendor-neutrality: every emitted artifact verifies
//! with stock tooling, no Attestrum install). Decision `cyclonedx-mlbom-shape`,
//! 2026-05-30 (multi-agent high-stakes protocol). The document shape is the
//! contract in `docs/diagrams/overview/cyclonedx-document-shape.md`.
//!
//! **The honesty invariant (the load-bearing decision).** A CycloneDX `hashes`
//! entry means "the digest of this component's bytes" — a stock tool will
//! recompute it. So `hashes` carries **only the SHA-256** of `manifest.parquet`,
//! which is exactly the Sigstore-signed in-toto **subject digest**
//! (`verify.rs:44-46`; `predicate.rs:36-38`). That value is true, signed, and
//! independently recomputable (`sha256sum manifest.parquet` matches). The BLAKE3
//! **Merkle root** is a tree root, not a flat digest — it is **never** placed in
//! `hashes` (a verifier recomputing it would get a different value and wrongly
//! contradict the artifact). All BLAKE3 values live in namespaced `properties`,
//! so the document never shows two BLAKE3 values in two semantic roles.
//!
//! **Determinism (CLAUDE.md §7).** No `serialNumber` (omitted — nothing keys on
//! it; avoids a `uuid` dep). A deterministic `metadata.timestamp` derived from
//! [`CycloneDxPlan::source_date_epoch`] via `jiff::Timestamp::from_second` (no
//! wall-clock, Croissant-consistent). Content-derived `bom-ref`. Sorted-key
//! serialization via [`attestrum_attest::deterministic_json`].
//!
//! **Vendor-neutrality (CLAUDE.md §12).** "attestrum" appears only as the
//! `metadata.tools` component name, in the §4-protected predicate URI under
//! `externalReferences`, and as the explicit `attestrum:` vendor-extension
//! `properties` namespace — never in vendor-neutral identity fields
//! (`supplier`, `name`, `governance`). The dataset `supplier` is the corpus
//! publisher (the Attestrum GitHub Actions workflow identity for demos — never
//! an individual; CLAUDE-LOCAL §A9), via `--publisher`, omitted when absent.
//! `authors` is omitted (personal contact fields). **Honest omission
//! throughout:** `licenses`, `supplier`, `governance.owners`, `classification`
//! are emitted only when their input is supplied — never fabricated (the
//! Croissant `license:"unknown"` precedent).

use crate::{AttestrumEmitError, CycloneDxPlan};
use serde_json::json;

/// The CycloneDX spec version this emitter pins. ECMA-424 (ratified 1.6); the
/// unratified 1.7 is forbidden (decision A, §A4.1.2 version pin).
const CYCLONEDX_SPEC_VERSION: &str = "1.6";

/// The §4-protected in-toto predicate type URI for the training-corpus
/// attestation. Referenced here in the attestation `externalReference` comment,
/// never redefined (CLAUDE.md §4 / §12). Matches the value the Croissant
/// emitter and `attestrum-attest` use.
const ATTESTRUM_PREDICATE_URI: &str = "https://attestrum.com/attestation/training-corpus/v0.3";

/// Namespaced `properties` keys (the explicit `attestrum:` vendor-extension
/// namespace, parallel to the Croissant emitter's `attestrum:` JSON-LD prefix).
/// The Merkle root and corpus statistics live here — the Merkle root NEVER in
/// `hashes` (the disqualified C1 option from the decision).
const PROP_MERKLE_ROOT_BLAKE3: &str = "attestrum:merkle.root.blake3";
const PROP_CORPUS_LEAF_COUNT: &str = "attestrum:corpus.leafCount";
const PROP_CORPUS_TOTAL_BYTES: &str = "attestrum:corpus.totalBytes";

/// Render a CycloneDX 1.6 ML-BOM as a canonical-form JSON string.
///
/// The emitted file validates against `sbom-utility validate` with zero errors
/// and zero warnings. See module docs for the honesty invariant (SHA-256 only
/// in `hashes`, all BLAKE3 in `properties`), the determinism contract, and the
/// vendor-neutrality placement rules.
pub fn render(plan: &CycloneDxPlan) -> Result<String, AttestrumEmitError> {
    let timestamp = jiff::Timestamp::from_second(plan.source_date_epoch)
        .map_err(|e| {
            AttestrumEmitError::CycloneDx(format!(
                "source_date_epoch {} out of range: {e}",
                plan.source_date_epoch
            ))
        })?
        .to_string();

    let bom_ref = format!("dataset-{}-{}", plan.dataset_name, plan.version);

    // The data component — the sealed corpus. `hashes` carries ONLY the
    // SHA-256 (the signed manifest subject digest); the BLAKE3 Merkle root
    // lives in `properties` below, never here.
    let mut component = json!({
        "type": "data",
        "bom-ref": bom_ref,
        "name": plan.dataset_name,
        "version": plan.version,
        "hashes": [
            { "alg": "SHA-256", "content": plan.manifest_sha256_hex }
        ],
        "externalReferences": [
            {
                "type": "attestation",
                "url": plan.bundle_path_in_repo,
                "comment": format!("in-toto training-corpus predicate {ATTESTRUM_PREDICATE_URI}"),
            },
            {
                "type": "distribution",
                "url": plan.manifest_path_in_repo,
            },
        ],
        "properties": [
            { "name": PROP_MERKLE_ROOT_BLAKE3, "value": plan.merkle_root_blake3_hex },
            { "name": PROP_CORPUS_LEAF_COUNT, "value": plan.manifest_stats.leaf_count.to_string() },
            { "name": PROP_CORPUS_TOTAL_BYTES, "value": plan.manifest_stats.total_bytes.to_string() },
        ],
        // The typed-dataset assertion — the load-bearing invariant that earns
        // the `data` representation. ALWAYS present.
        "data": [ component_data(plan) ],
    });

    // Honest omission: emit `supplier` only when a publisher is supplied; never
    // synthesize an owner. The publisher is the corpus publisher org (the
    // Attestrum GHA identity for demos — never an individual; §A9).
    if let Some(publisher) = &plan.publisher {
        component["supplier"] = json!({ "name": publisher });
    }

    // Honest omission: emit `licenses` only when a license is resolved. A valid
    // SPDX id → `license.id`; the honest "unknown" token / any non-SPDX string
    // → `license.name` (id requires a valid SPDX id). Reuses the same resolved
    // value the Croissant + README path produces so the artifacts agree.
    if let Some(license) = &plan.license {
        component["licenses"] = json!([{ "license": license_object(license) }]);
    }

    let document = json!({
        "bomFormat": "CycloneDX",
        "specVersion": CYCLONEDX_SPEC_VERSION,
        "metadata": {
            "timestamp": timestamp,
            // "attestrum" as the generating tool — one of the three allowed
            // structural placements (tool name, predicate URI, attestrum:
            // property keys).
            "tools": {
                "components": [
                    { "type": "application", "name": "attestrum" }
                ]
            },
            "component": component,
        },
    });

    attestrum_attest::deterministic_json(&document)
        .map_err(|e| AttestrumEmitError::CycloneDx(format!("serialize: {e}")))
}

/// Build the single `componentData{type:"dataset"}` object. `governance.owners`
/// is emitted only with a publisher; `classification` only when supplied —
/// honest omission, never fabricated. `contents`/`custodians`/`stewards` are
/// never emitted (decision B).
fn component_data(plan: &CycloneDxPlan) -> serde_json::Value {
    let mut data = json!({
        "type": "dataset",
        "name": plan.dataset_name,
    });
    if let Some(publisher) = &plan.publisher {
        data["governance"] = json!({
            "owners": [ { "organization": { "name": publisher } } ]
        });
    }
    if let Some(classification) = &plan.classification {
        data["classification"] = json!(classification);
    }
    data
}

/// Map a resolved license string to a CycloneDX `license` object. A valid SPDX
/// id uses `id`; anything else (the honest `"unknown"` token, a non-SPDX
/// expression) uses `name` — the CycloneDX schema only accepts a registered
/// SPDX id under `id`, so a non-SPDX value there would fail validation.
fn license_object(license: &str) -> serde_json::Value {
    if is_spdx_id(license) {
        json!({ "id": license })
    } else {
        json!({ "name": license })
    }
}

/// Whether `value` is a recognised single SPDX license id. The CLI resolves the
/// corpus license to either a real SPDX id or the honest `"unknown"` token (the
/// Croissant precedent), so this distinguishes those two cases. Deliberately a
/// small allow-list of the ids Attestrum publishes under plus the common
/// permissive/CC set — a full SPDX table is unwarranted (CLAUDE.md §14 eager
/// generalization); extend when a real corpus needs an id not listed.
fn is_spdx_id(value: &str) -> bool {
    const KNOWN_SPDX_IDS: &[&str] = &[
        "Apache-2.0",
        "MIT",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "MPL-2.0",
        "Unlicense",
        "CC0-1.0",
        "CC-BY-4.0",
        "CC-BY-SA-4.0",
        "CC-BY-NC-4.0",
        "CC-BY-ND-4.0",
        "CC-BY-NC-SA-4.0",
        "ODC-By-1.0",
        "ODbL-1.0",
    ];
    KNOWN_SPDX_IDS.contains(&value)
}
