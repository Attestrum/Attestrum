//! S5-D3 E4 — integration tests for the Croissant 1.0 JSON-LD emitter.
//!
//! Six tests pin the structural and byte-level contracts:
//!
//! 1. `render_matches_golden` — byte-identity against `tests/fixtures/
//!    croissant-5entry.golden.json`. Regenerate via
//!    `ATTESTRUM_REGEN_CROISSANT_GOLDEN=1 cargo test -p attestrum-emit
//!    --test croissant render_matches_golden`.
//! 2. `render_is_deterministic_across_runs` — byte-equal on repeated calls.
//! 3. `render_includes_four_attestrum_extension_uris` — four extension URIs
//!    locked at E4 per the roadmap §E4 acceptance criterion.
//! 4. `render_has_croissant_context_and_dataset_type` — structural shape.
//! 5. `render_omits_license_when_none` — caller signals "don't know" / "multi-
//!    license" by passing None; emitter does not synthesize a value.
//! 6. `render_rejects_out_of_range_source_date_epoch` — `Croissant` error
//!    variant when `jiff::Timestamp::from_second` rejects the input.

use attestrum_emit::{render_croissant, CroissantPlan, ManifestStats};

const GOLDEN_PATH: &str = "tests/fixtures/croissant-5entry.golden.json";

fn fixture_plan() -> CroissantPlan {
    CroissantPlan {
        dataset_name: "my-org/my-dataset".to_string(),
        manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
        bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
        merkle_root_path_in_repo: "attestrum/merkle.root".to_string(),
        manifest_stats: ManifestStats {
            leaf_count: 5,
            total_bytes: 1024,
        },
        // 2023-11-14T22:13:20Z — pinned for byte-identity across runs + 4-target CI matrix.
        source_date_epoch: 1_700_000_000,
        license_spdx: Some("Apache-2.0".to_string()),
    }
}

#[test]
fn render_matches_golden() {
    let actual = render_croissant(&fixture_plan()).expect("render");
    if std::env::var("ATTESTRUM_REGEN_CROISSANT_GOLDEN").is_ok() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        panic!(
            "golden regenerated at {GOLDEN_PATH} — re-run without \
             ATTESTRUM_REGEN_CROISSANT_GOLDEN to verify"
        );
    }
    let golden = std::fs::read_to_string(GOLDEN_PATH).expect("read golden");
    assert_eq!(
        actual, golden,
        "Croissant output drifted from golden — regen via \
         ATTESTRUM_REGEN_CROISSANT_GOLDEN=1 if intentional"
    );
}

#[test]
fn render_is_deterministic_across_runs() {
    let plan = fixture_plan();
    let a = render_croissant(&plan).expect("render a");
    let b = render_croissant(&plan).expect("render b");
    assert_eq!(a, b, "render_croissant must be byte-deterministic");
}

#[test]
fn render_includes_four_attestrum_extension_uris() {
    let out = render_croissant(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    let prov = &v["attestrum:provenance"];
    assert_eq!(
        prov["attestrum:predicate"], "https://attestrum.com/attestation/training-corpus/v0.3",
        "predicate URI must match implemented training-corpus/v0.3"
    );
    assert_eq!(prov["attestrum:manifest"], "attestrum/manifest.parquet");
    assert_eq!(prov["attestrum:merkleRoot"], "attestrum/merkle.root");
    assert_eq!(prov["attestrum:bundle"], "attestrum/bundle.sigstore.json");
}

#[test]
fn render_has_croissant_context_and_dataset_type() {
    let out = render_croissant(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(v["@type"], "sc:Dataset");
    assert_eq!(v["cr:isLiveDataset"], false);
    let ctx = v["@context"].as_array().expect("@context must be array");
    assert!(
        ctx.iter()
            .any(|c| c.as_str() == Some("http://mlcommons.org/croissant/1.0/context.json")),
        "@context must reference the Croissant 1.0 context URL"
    );
    assert!(
        ctx.iter()
            .any(|c| c.get("attestrum").and_then(|a| a.as_str())
                == Some("https://attestrum.com/croissant/v0.1/")),
        "@context must define the attestrum namespace prefix"
    );
}

#[test]
fn render_omits_license_when_none() {
    let mut plan = fixture_plan();
    plan.license_spdx = None;
    let out = render_croissant(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert!(
        v.get("license").is_none(),
        "license field must be absent when plan.license_spdx is None"
    );
}

#[test]
fn render_rejects_out_of_range_source_date_epoch() {
    let mut plan = fixture_plan();
    plan.source_date_epoch = i64::MAX;
    let err = render_croissant(&plan).expect_err("i64::MAX is out of jiff::Timestamp range");
    assert!(
        matches!(err, attestrum_emit::AttestrumEmitError::Croissant(_)),
        "expected Croissant variant, got {err:?}"
    );
}
