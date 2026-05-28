//! S5-D3 E5 — integration tests for the dataset card README emitter.
//!
//! Seven tests pin the structural and byte-level contracts:
//!
//! 1. `render_matches_golden` — byte-identity against `tests/fixtures/
//!    readme-5entry.golden.md`. Regenerate via
//!    `ATTESTRUM_REGEN_README_GOLDEN=1 cargo test -p attestrum-emit
//!    --test dataset_card render_matches_golden`.
//! 2. `render_is_deterministic_across_runs` — byte-equal on repeated calls.
//! 3. `render_emits_yaml_frontmatter_delimiters` — starts with `---\n`,
//!    contains the frontmatter-closing `\n---\n\n`.
//! 4. `render_includes_five_attestrum_extension_keys` — all five
//!    `attestrum:` block keys present per the locked E5 contract.
//! 5. `render_emits_license_details_only_when_mixed` — covers both branches
//!    of the brief's "single SPDX vs mixed" license handling.
//! 6. `render_appends_three_required_tags` — the three Hub-canonical tags
//!    are appended regardless of caller-supplied tags.
//! 7. `render_rejects_empty_pretty_name` — validation rejects empty
//!    required fields with `AttestrumEmitError::Readme(_)`.

use attestrum_emit::{render_readme, DatasetCardPlan, ManifestStats};

const GOLDEN_PATH: &str = "tests/fixtures/readme-5entry.golden.md";

fn fixture_plan() -> DatasetCardPlan {
    DatasetCardPlan {
        pretty_name: "My Dataset (v0.1)".to_string(),
        license_spdx: "Apache-2.0".to_string(),
        language: vec!["en".to_string()],
        task_categories: vec!["text-generation".to_string()],
        size_category: "n<1K".to_string(),
        tags: vec!["example".to_string()],
        dataset_name: "my-org/my-dataset".to_string(),
        manifest_stats: ManifestStats {
            leaf_count: 5,
            total_bytes: 1024,
        },
        verify_url:
            "https://huggingface.co/datasets/my-org/my-dataset/blob/main/attestrum/verify.html"
                .to_string(),
    }
}

#[test]
fn render_matches_golden() {
    let actual = render_readme(&fixture_plan()).expect("render");
    if std::env::var("ATTESTRUM_REGEN_README_GOLDEN").is_ok() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        panic!(
            "golden regenerated at {GOLDEN_PATH} — re-run without \
             ATTESTRUM_REGEN_README_GOLDEN to verify"
        );
    }
    let golden = std::fs::read_to_string(GOLDEN_PATH).expect("read golden");
    assert_eq!(
        actual, golden,
        "README output drifted from golden — regen via \
         ATTESTRUM_REGEN_README_GOLDEN=1 if intentional"
    );
}

#[test]
fn render_is_deterministic_across_runs() {
    let plan = fixture_plan();
    let a = render_readme(&plan).expect("render a");
    let b = render_readme(&plan).expect("render b");
    assert_eq!(a, b, "render_readme must be byte-deterministic");
}

#[test]
fn render_emits_yaml_frontmatter_delimiters() {
    let out = render_readme(&fixture_plan()).expect("render");
    assert!(
        out.starts_with("---\n"),
        "README must open with YAML frontmatter delimiter"
    );
    assert!(
        out.contains("\n---\n\n"),
        "README must close YAML frontmatter with `---` followed by blank line"
    );
}

#[test]
fn render_includes_five_attestrum_extension_keys() {
    let out = render_readme(&fixture_plan()).expect("render");
    // Keys appear inside the `attestrum:` block, each indented with two spaces.
    assert!(out.contains("  bundle:"), "missing attestrum.bundle key");
    assert!(
        out.contains("  manifest:"),
        "missing attestrum.manifest key"
    );
    assert!(
        out.contains("  merkle_root:"),
        "missing attestrum.merkle_root key"
    );
    assert!(
        out.contains("  predicate:"),
        "missing attestrum.predicate key"
    );
    assert!(
        out.contains("  verify_url:"),
        "missing attestrum.verify_url key"
    );
    // Predicate URI must match the implemented training-corpus/v0.3.
    assert!(
        out.contains("https://attestrum.com/attestation/training-corpus/v0.3"),
        "predicate URI must match implemented training-corpus/v0.3"
    );
}

#[test]
fn render_emits_license_details_only_when_mixed() {
    let mut plan = fixture_plan();
    plan.license_spdx = "mixed".to_string();
    let out = render_readme(&plan).expect("render mixed");
    assert!(
        out.contains("license_details: \"see attestrum/license-inventory.json\""),
        "mixed-license must emit license_details pointer"
    );

    plan.license_spdx = "MIT".to_string();
    let out = render_readme(&plan).expect("render single");
    assert!(
        !out.contains("license_details"),
        "single-license must NOT emit license_details"
    );
}

#[test]
fn render_appends_three_required_tags() {
    let out = render_readme(&fixture_plan()).expect("render");
    assert!(
        out.contains("- \"attestrum-provenance\""),
        "missing required tag attestrum-provenance"
    );
    assert!(
        out.contains("- \"sigstore-signed\""),
        "missing required tag sigstore-signed"
    );
    assert!(
        out.contains("- \"croissant\""),
        "missing required tag croissant"
    );
    // Caller-supplied tag still present + appears before the required tags.
    let example_pos = out.find("- \"example\"").expect("caller tag missing");
    let provenance_pos = out
        .find("- \"attestrum-provenance\"")
        .expect("required tag missing");
    assert!(
        example_pos < provenance_pos,
        "caller tag must precede required tags"
    );
}

#[test]
fn render_rejects_empty_pretty_name() {
    let mut plan = fixture_plan();
    plan.pretty_name = String::new();
    assert!(
        matches!(
            render_readme(&plan).unwrap_err(),
            attestrum_emit::AttestrumEmitError::Readme(_)
        ),
        "empty pretty_name must error with Readme variant"
    );
}
