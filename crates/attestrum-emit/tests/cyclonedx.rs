//! Integration tests for the CycloneDX 1.6 ML-BOM emitter.
//!
//! Pins the structural, honesty, and byte-level contracts (decision
//! `cyclonedx-mlbom-shape`, 2026-05-30 — the emitted file must validate against
//! the public CycloneDX validator `sbom-utility` with zero errors / zero
//! warnings):
//!
//! 1. `render_matches_golden` — byte-identity against `tests/fixtures/
//!    cyclonedx-5entry.golden.json` (the fully-supplied shape). Regenerate via
//!    `ATTESTRUM_REGEN_CYCLONEDX_GOLDEN=1 cargo test -p attestrum-emit --test
//!    cyclonedx render_matches_golden`.
//! 2. `render_is_deterministic_across_runs` — TWO renders byte-equal (the
//!    determinism proof, not validating one render).
//! 3. `render_has_required_root_and_component_shape` — `bomFormat`/`specVersion`,
//!    `metadata.component.type=="data"`, `data[0].type=="dataset"`.
//! 4. `hashes_carries_exactly_one_sha256_no_blake3` — the honesty invariant:
//!    one SHA-256 in `hashes`, no BLAKE3 anywhere in `hashes`, the Merkle root
//!    present only under `properties[attestrum:merkle.root.blake3]`.
//! 5. `render_omits_optional_fields_when_none` — honest omission of `licenses`,
//!    `supplier`, `governance`, `classification`.
//! 6. `license_maps_spdx_to_id_and_unknown_to_name` — SPDX → `license.id`,
//!    non-SPDX/`"unknown"` → `license.name`.
//! 7. `attestrum_string_appears_only_in_allowed_placements` — vendor-neutrality
//!    guard: every "attestrum" occurrence is the tool name, the predicate URI,
//!    an `attestrum:` property key, or a repo-relative `attestrum/` artifact
//!    path — never a vendor-neutral identity field (`supplier`/`name`/etc.).
//! 8. `render_rejects_out_of_range_source_date_epoch` — `CycloneDx` error variant.

use attestrum_emit::{render_cyclonedx, CycloneDxPlan, ManifestStats};

const GOLDEN_PATH: &str = "tests/fixtures/cyclonedx-5entry.golden.json";
const PREDICATE_URI: &str = "https://attestrum.com/attestation/training-corpus/v0.3";

/// Fully-supplied fixture — the golden shape with license + publisher +
/// classification all present.
fn fixture_plan() -> CycloneDxPlan {
    CycloneDxPlan {
        dataset_name: "my-org/my-dataset".to_string(),
        version: "1.0.0".to_string(),
        // 2023-11-14T22:13:20Z — pinned for byte-identity across runs + 4-target CI matrix.
        source_date_epoch: 1_700_000_000,
        manifest_sha256_hex: "a".repeat(64),
        merkle_root_blake3_hex: "b".repeat(64),
        manifest_stats: ManifestStats {
            leaf_count: 5,
            total_bytes: 1024,
        },
        license: Some("Apache-2.0".to_string()),
        publisher: Some("my-org".to_string()),
        classification: Some("public".to_string()),
        manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
        bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
    }
}

#[test]
fn render_matches_golden() {
    let actual = render_cyclonedx(&fixture_plan()).expect("render");
    if std::env::var("ATTESTRUM_REGEN_CYCLONEDX_GOLDEN").is_ok() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        panic!(
            "golden regenerated at {GOLDEN_PATH} — re-run without \
             ATTESTRUM_REGEN_CYCLONEDX_GOLDEN to verify"
        );
    }
    let golden = std::fs::read_to_string(GOLDEN_PATH).expect("read golden");
    assert_eq!(
        actual, golden,
        "CycloneDX output drifted from golden — regen via \
         ATTESTRUM_REGEN_CYCLONEDX_GOLDEN=1 if intentional"
    );
}

#[test]
fn render_is_deterministic_across_runs() {
    let plan = fixture_plan();
    let a = render_cyclonedx(&plan).expect("render a");
    let b = render_cyclonedx(&plan).expect("render b");
    assert_eq!(a, b, "render_cyclonedx must be byte-deterministic");
}

#[test]
fn render_has_required_root_and_component_shape() {
    let out = render_cyclonedx(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");

    assert_eq!(v["bomFormat"], "CycloneDX");
    assert_eq!(v["specVersion"], "1.6");
    assert!(
        v.get("serialNumber").is_none(),
        "serialNumber omitted for determinism (no uuid dep)"
    );

    let component = &v["metadata"]["component"];
    assert_eq!(component["type"], "data", "outer component is type=data");
    assert_eq!(component["name"], "my-org/my-dataset");
    assert_eq!(component["version"], "1.0.0");
    assert_eq!(component["bom-ref"], "dataset-my-org/my-dataset-1.0.0");

    let data0 = &component["data"][0];
    assert_eq!(
        data0["type"], "dataset",
        "componentData.type=dataset is the load-bearing typed assertion"
    );
    assert_eq!(data0["name"], "my-org/my-dataset");

    // tools.components carries the only structural tool identity.
    assert_eq!(v["metadata"]["tools"]["components"][0]["name"], "attestrum");
    // deterministic timestamp from source_date_epoch (no wall-clock).
    assert_eq!(v["metadata"]["timestamp"], "2023-11-14T22:13:20Z");
}

#[test]
fn hashes_carries_exactly_one_sha256_no_blake3() {
    let out = render_cyclonedx(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");

    let hashes = v["metadata"]["component"]["hashes"]
        .as_array()
        .expect("hashes is an array");
    assert_eq!(hashes.len(), 1, "exactly one hash entry");
    assert_eq!(hashes[0]["alg"], "SHA-256", "the one hash is SHA-256");
    assert_eq!(hashes[0]["content"], "a".repeat(64));

    // The honesty invariant: NO BLAKE3 anywhere in `hashes`.
    for h in hashes {
        let alg = h["alg"].as_str().unwrap_or("");
        assert!(
            !alg.to_ascii_uppercase().contains("BLAKE"),
            "no BLAKE3 in hashes — alg was {alg:?}"
        );
    }

    // The Merkle root lives ONLY in the namespaced property, and its value
    // appears nowhere in `hashes`.
    let props = v["metadata"]["component"]["properties"]
        .as_array()
        .expect("properties array");
    let merkle = props
        .iter()
        .find(|p| p["name"] == "attestrum:merkle.root.blake3")
        .expect("merkle root property present");
    assert_eq!(merkle["value"], "b".repeat(64));
    let hashes_str = serde_json::to_string(&v["metadata"]["component"]["hashes"]).unwrap();
    assert!(
        !hashes_str.contains(&"b".repeat(64)),
        "Merkle root BLAKE3 must never appear in hashes"
    );

    // Corpus stats are carried as string-valued properties.
    let leaf = props
        .iter()
        .find(|p| p["name"] == "attestrum:corpus.leafCount")
        .expect("leafCount property");
    assert_eq!(leaf["value"], "5");
    let bytes = props
        .iter()
        .find(|p| p["name"] == "attestrum:corpus.totalBytes")
        .expect("totalBytes property");
    assert_eq!(bytes["value"], "1024");
}

#[test]
fn render_carries_external_references() {
    let out = render_cyclonedx(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    let refs = v["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("externalReferences array");

    let attestation = refs
        .iter()
        .find(|r| r["type"] == "attestation")
        .expect("attestation ref");
    assert_eq!(attestation["url"], "attestrum/bundle.sigstore.json");
    assert!(
        attestation["comment"]
            .as_str()
            .unwrap_or("")
            .contains(PREDICATE_URI),
        "attestation comment references the predicate URI"
    );

    let distribution = refs
        .iter()
        .find(|r| r["type"] == "distribution")
        .expect("distribution ref");
    assert_eq!(distribution["url"], "attestrum/manifest.parquet");
}

#[test]
fn render_omits_optional_fields_when_none() {
    let mut plan = fixture_plan();
    plan.license = None;
    plan.publisher = None;
    plan.classification = None;
    let out = render_cyclonedx(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");

    let component = &v["metadata"]["component"];
    assert!(
        component.get("licenses").is_none(),
        "licenses omitted when None"
    );
    assert!(
        component.get("supplier").is_none(),
        "supplier omitted when no publisher"
    );
    let data0 = &component["data"][0];
    assert!(
        data0.get("governance").is_none(),
        "governance omitted when no publisher"
    );
    assert!(
        data0.get("classification").is_none(),
        "classification omitted when None"
    );
    // The typed-dataset assertion is still always present.
    assert_eq!(data0["type"], "dataset");
}

#[test]
fn license_maps_spdx_to_id_and_unknown_to_name() {
    // SPDX id → license.id
    let mut plan = fixture_plan();
    plan.license = Some("MIT".to_string());
    let out = render_cyclonedx(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    let lic = &v["metadata"]["component"]["licenses"][0]["license"];
    assert_eq!(lic["id"], "MIT");
    assert!(lic.get("name").is_none(), "SPDX id uses id, not name");

    // The honest "unknown" token (non-SPDX) → license.name
    plan.license = Some("unknown".to_string());
    let out = render_cyclonedx(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    let lic = &v["metadata"]["component"]["licenses"][0]["license"];
    assert_eq!(lic["name"], "unknown");
    assert!(
        lic.get("id").is_none(),
        "non-SPDX value uses name, not id (id requires a valid SPDX id)"
    );
}

#[test]
fn attestrum_string_appears_only_in_allowed_placements() {
    // Vendor-neutrality guard (decision D). Use the no-publisher fixture so the
    // only "attestrum" occurrences are the allowed placements: the tool name,
    // the predicate URI, `attestrum:` property keys, and the repo-relative
    // `attestrum/` artifact paths (the committed-directory convention, the same
    // one the Croissant emitter references) — never a vendor-neutral identity
    // field.
    let mut plan = fixture_plan();
    plan.publisher = None;
    let out = render_cyclonedx(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");

    // No "attestrum" in the vendor-neutral identity fields.
    let component = &v["metadata"]["component"];
    assert!(!component["name"].as_str().unwrap().contains("attestrum"));
    assert!(!component["version"].as_str().unwrap().contains("attestrum"));
    assert!(component.get("supplier").is_none());

    // Walk EVERY string in the document; each occurrence of "attestrum"
    // (case-insensitive) must be explainable by an allowed placement.
    let mut occurrences = 0usize;
    walk_strings(&v, &mut |s| {
        if s.to_ascii_lowercase().contains("attestrum") {
            occurrences += 1;
            let allowed = s == "attestrum"                       // tool name
                || s.contains(PREDICATE_URI)                     // predicate URI (in a comment)
                || s.starts_with("attestrum:")                   // namespaced property key
                || s.starts_with("attestrum/"); // committed-artifact path
            assert!(
                allowed,
                "\"attestrum\" appeared in a disallowed placement: {s:?}"
            );
        }
    });
    assert!(
        occurrences >= 4,
        "expected the tool name + predicate URI + 3 property keys, got {occurrences}"
    );
}

#[test]
fn render_rejects_out_of_range_source_date_epoch() {
    let mut plan = fixture_plan();
    plan.source_date_epoch = i64::MAX;
    let err = render_cyclonedx(&plan).expect_err("i64::MAX is out of jiff::Timestamp range");
    assert!(
        matches!(err, attestrum_emit::AttestrumEmitError::CycloneDx(_)),
        "expected CycloneDx variant, got {err:?}"
    );
}

/// Recursively visit every JSON string value AND object key in `v`, calling
/// `f` on each. Used by the vendor-neutrality guard to audit every placement.
fn walk_strings(v: &serde_json::Value, f: &mut impl FnMut(&str)) {
    match v {
        serde_json::Value::String(s) => f(s),
        serde_json::Value::Array(arr) => {
            for item in arr {
                walk_strings(item, f);
            }
        }
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                f(k);
                walk_strings(val, f);
            }
        }
        _ => {}
    }
}
