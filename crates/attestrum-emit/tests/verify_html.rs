//! S5-D3 E6 — integration tests for the verify.html stub renderer.
//!
//! Two tests pin the byte-level contract:
//!
//! 1. `render_matches_golden` — byte-identity against `tests/fixtures/
//!    verify_html-stub.golden.html`. Regenerate via
//!    `ATTESTRUM_REGEN_VERIFY_HTML_GOLDEN=1 cargo test -p attestrum-emit
//!    --test verify_html render_matches_golden`. The 4-target determinism
//!    CI matrix is the load-bearing watch — a divergence in any target
//!    fails the build.
//! 2. `render_is_deterministic_across_runs` — byte-equal on repeated
//!    calls within a single process (catches in-process nondeterminism
//!    like map iteration that the golden test alone wouldn't catch on
//!    short timescales).

use attestrum_emit::{render_verify_html_stub, ManifestStats, VerifyHtmlPlan};

const GOLDEN_PATH: &str = "tests/fixtures/verify_html-stub.golden.html";

fn fixture_plan() -> VerifyHtmlPlan {
    VerifyHtmlPlan {
        dataset_name: "my-org/my-dataset".to_string(),
        certificate_identity:
            "https://github.com/my-org/my-dataset/.github/workflows/build.yml@refs/heads/main"
                .to_string(),
        certificate_oidc_issuer: "https://token.actions.githubusercontent.com".to_string(),
        bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
        manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
        manifest_stats: ManifestStats {
            leaf_count: 1234,
            total_bytes: 5_678_900,
        },
    }
}

#[test]
fn render_matches_golden() {
    let actual = render_verify_html_stub(&fixture_plan()).expect("render");
    if std::env::var("ATTESTRUM_REGEN_VERIFY_HTML_GOLDEN").is_ok() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        panic!(
            "golden regenerated at {GOLDEN_PATH} — re-run without \
             ATTESTRUM_REGEN_VERIFY_HTML_GOLDEN to verify"
        );
    }
    let golden = std::fs::read_to_string(GOLDEN_PATH).expect("read golden");
    assert_eq!(
        actual, golden,
        "verify.html stub diverges from golden — run with \
         ATTESTRUM_REGEN_VERIFY_HTML_GOLDEN=1 to update if intentional"
    );
}

#[test]
fn render_is_deterministic_across_runs() {
    let a = render_verify_html_stub(&fixture_plan()).expect("render a");
    let b = render_verify_html_stub(&fixture_plan()).expect("render b");
    assert_eq!(a, b);
}
