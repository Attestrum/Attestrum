//! Integration tests for the Croissant 1.0 JSON-LD emitter.
//!
//! Pins the structural and byte-level contracts (decision
//! `croissant-context-conformance`, 2026-05-30 — the emitted file must validate
//! against the public `mlcroissant` validator):
//!
//! 1. `render_matches_golden` — byte-identity against `tests/fixtures/
//!    croissant-5entry.golden.json` (the fully-supplied zero/zero shape).
//!    Regenerate via `ATTESTRUM_REGEN_CROISSANT_GOLDEN=1 cargo test -p
//!    attestrum-emit --test croissant render_matches_golden`.
//! 2. `render_is_deterministic_across_runs` — byte-equal on repeated calls.
//! 3. `render_includes_four_attestrum_extension_uris` — extension URIs intact.
//! 4. `render_has_dict_context_dataset_type_and_conforms_to` — `@context` is a
//!    dict, `@type: Dataset`, `conformsTo` 1.0, unqualified `isLiveDataset`/
//!    `recordSet`.
//! 5. `context_has_exactly_the_standard_keyset_plus_attestrum` — 36 standard
//!    v1.0 keys + attestrum; guards mlcroissant missing-key warnings.
//! 6. `render_emits_version_and_cite_as_when_supplied` — recommended fields +
//!    `datePublished`.
//! 7. `render_omits_recommended_fields_when_none` — honest omission (no
//!    fabrication) when license/version/citeAs are absent.
//! 8. `render_rejects_out_of_range_source_date_epoch` — `Croissant` error
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
        // Fully-supplied fixture — the golden is the zero-errors/zero-warnings
        // shape (license + version + citeAs all present).
        version: Some("1.0.0".to_string()),
        cite_as: Some("Example, A. (2025). My Dataset. https://example.org/ds".to_string()),
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
fn render_has_dict_context_dataset_type_and_conforms_to() {
    let out = render_croissant(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");

    // @type is the bare `Dataset` (resolved via @vocab), NOT `sc:Dataset`.
    assert_eq!(v["@type"], "Dataset");
    // conformsTo declares Croissant 1.0 so mlcroissant treats the file as v1.0.
    assert_eq!(v["conformsTo"], "http://mlcommons.org/croissant/1.0");
    // Unqualified keys per the standard context (not cr:-prefixed).
    assert_eq!(v["isLiveDataset"], false);
    assert!(
        v["recordSet"].is_array(),
        "recordSet must be a (possibly empty) array"
    );
    assert!(
        v.get("cr:isLiveDataset").is_none() && v.get("cr:recordSet").is_none(),
        "keys must use the unqualified standard form, not cr:-prefixed"
    );

    // @context is a DICT (mlcroissant hard-requires this), with the standard
    // prefixes and the attestrum extension key.
    let ctx = v["@context"]
        .as_object()
        .expect("@context must be a dict (array fails mlcroissant get_context)");
    for key in ["sc", "cr", "dct", "conformsTo", "recordSet", "fileObject"] {
        assert!(
            ctx.contains_key(key),
            "@context missing standard key `{key}`"
        );
    }
    assert_eq!(
        ctx["sc"], "https://schema.org/",
        "sc: must map to schema.org so @type Dataset resolves"
    );
    assert_eq!(
        ctx["attestrum"], "https://attestrum.com/croissant/v0.1/",
        "@context must define the attestrum namespace prefix"
    );
}

#[test]
fn context_has_exactly_the_standard_keyset_plus_attestrum() {
    // Guards against drift from mlcroissant's make_context() for Croissant 1.0:
    // 36 standard keys + the single attestrum extension key = 37.
    let out = render_croissant(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    let ctx = v["@context"].as_object().expect("@context dict");
    assert!(
        ctx.contains_key("attestrum"),
        "attestrum extension key must be present"
    );
    assert_eq!(
        ctx.len(),
        37,
        "expected 36 standard v1.0 keys + attestrum; mlcroissant warns on any \
         missing standard key — regenerate from make_context() if Croissant \
         version is bumped"
    );
}

#[test]
fn render_emits_version_and_cite_as_when_supplied() {
    let out = render_croissant(&fixture_plan()).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(v["version"], "1.0.0", "version must be the supplied semver");
    assert_eq!(
        v["citeAs"], "Example, A. (2025). My Dataset. https://example.org/ds",
        "citeAs must be the supplied citation"
    );
    // datePublished is derived from source_date_epoch alongside dateCreated.
    assert_eq!(v["datePublished"], v["dateCreated"]);
}

#[test]
fn render_omits_recommended_fields_when_none() {
    // The honest default: omit (not fabricate) version/citeAs/license when the
    // publisher supplies nothing. mlcroissant emits a benign recommended-field
    // warning for each — never a fabricated value.
    let mut plan = fixture_plan();
    plan.license_spdx = None;
    plan.version = None;
    plan.cite_as = None;
    let out = render_croissant(&plan).expect("render");
    let v: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert!(v.get("license").is_none(), "license omitted when None");
    assert!(v.get("version").is_none(), "version omitted when None");
    assert!(v.get("citeAs").is_none(), "citeAs omitted when None");
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
