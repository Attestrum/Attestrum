//! verify.html stub renderer.
//!
//! v0.1 ships a static HTML page (~2 KB, no JS, no CSS framework, no external
//! resources) that displays the Sigstore identity policy from the bundle's
//! leaf cert and suggests the stock `cosign verify-blob-attestation
//! --new-bundle-format` command. Visitors can verify the bundle without
//! Attestrum installed (CLAUDE.md §12 vendor neutrality).
//!
//! The real in-browser WASM verifier (cosign-lite) ships in v0.2 per founder
//! scope decision SD2 at D3 planning time.
//!
//! Determinism: pure `String::replace` substitution; no wall-clock; no map
//! iteration. Output is byte-identical across the 4-target CI matrix as long
//! as the input plan fields are byte-identical (they come from sorted-key
//! sources upstream — CLI flags + `attestrum_attest::extract_identity()`).
//!
//! HTML escaping: hand-rolled per CLAUDE.md §14 (no `html_escape` dep — five
//! substitution sites, single-purpose helper). Covers the five OWASP-
//! recommended HTML-context characters: `& < > " '`.

use crate::{AttestrumEmitError, VerifyHtmlPlan};
use attestrum_attest::TRAINING_CORPUS_PREDICATE_TYPE;

const HTML_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Verify — {dataset}</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 720px; margin: 2em auto; padding: 0 1em; color: #222; }
  h1 { font-size: 1.4em; }
  code, pre { background: #f4f4f4; padding: 0.2em 0.4em; border-radius: 3px; }
  pre { padding: 1em; overflow-x: auto; }
  .policy { border-left: 3px solid #555; padding-left: 1em; margin: 1em 0; }
  .policy dt { font-weight: bold; margin-top: 0.5em; }
</style>
</head>
<body>
<h1>Verify {dataset}</h1>
<p>This dataset was published with Attestrum. The Sigstore Bundle at
<code>{bundle_path}</code> attests that the sealed manifest at
<code>{manifest_path}</code> was signed by the identity below.</p>

<h2>Identity policy</h2>
<dl class="policy">
  <dt>Certificate identity (SAN)</dt>
  <dd><code>{san}</code></dd>
  <dt>OIDC issuer</dt>
  <dd><code>{issuer}</code></dd>
</dl>

<h2>Verify from the command line</h2>
<p>You can verify the bundle with stock Sigstore tooling — no Attestrum install required:</p>
<pre>cosign verify-blob-attestation \
  --new-bundle-format \
  --type {predicate_type} \
  --bundle {bundle_path} \
  --certificate-identity {san} \
  --certificate-oidc-issuer {issuer} \
  {manifest_path}</pre>

<p>For richer offline verification (Merkle-root re-derivation against the manifest), see <code>attestrum verify</code> at <a href="https://github.com/Attestrum/Attestrum">github.com/Attestrum/Attestrum</a>.</p>

<p><em>Attestrum v0.1 — static verify page. In-browser WASM verifier ships in v0.2.</em></p>
</body>
</html>
"#;

/// Render the static verify.html stub. See module docs for the determinism
/// contract and the vendor-neutral CLI-command rationale.
pub fn render(plan: &VerifyHtmlPlan) -> Result<String, AttestrumEmitError> {
    if plan.dataset_name.is_empty() {
        return Err(AttestrumEmitError::VerifyHtml(
            "dataset_name is required".to_string(),
        ));
    }
    if plan.certificate_identity.is_empty() {
        return Err(AttestrumEmitError::VerifyHtml(
            "certificate_identity is required".to_string(),
        ));
    }
    if plan.certificate_oidc_issuer.is_empty() {
        return Err(AttestrumEmitError::VerifyHtml(
            "certificate_oidc_issuer is required".to_string(),
        ));
    }
    if plan.bundle_path_in_repo.is_empty() {
        return Err(AttestrumEmitError::VerifyHtml(
            "bundle_path_in_repo is required".to_string(),
        ));
    }
    if plan.manifest_path_in_repo.is_empty() {
        return Err(AttestrumEmitError::VerifyHtml(
            "manifest_path_in_repo is required".to_string(),
        ));
    }

    let dataset = html_escape(&plan.dataset_name);
    let san = html_escape(&plan.certificate_identity);
    let issuer = html_escape(&plan.certificate_oidc_issuer);
    let bundle_path = html_escape(&plan.bundle_path_in_repo);
    let manifest_path = html_escape(&plan.manifest_path_in_repo);
    // Predicate-type URI is a fixed const (PROTECTED `attestrum-attest`); no
    // attacker-controlled input. Escape anyway for templating uniformity.
    let predicate_type = html_escape(TRAINING_CORPUS_PREDICATE_TYPE);

    // Sequential `.replace` instead of `format!` keeps the template literal
    // readable + avoids `format!`-arg-order surprises. The replace order
    // matters only if a substituted value contains another placeholder
    // verbatim — html_escape would escape any `{` / `}` since they're not
    // in the escape set, but the template placeholders are all
    // alphanumeric + underscore so no collision is possible.
    let out = HTML_TEMPLATE
        .replace("{dataset}", &dataset)
        .replace("{san}", &san)
        .replace("{issuer}", &issuer)
        .replace("{bundle_path}", &bundle_path)
        .replace("{manifest_path}", &manifest_path)
        .replace("{predicate_type}", &predicate_type);

    Ok(out)
}

/// Escape the five OWASP-recommended HTML-context characters. Non-ASCII
/// characters pass through as valid UTF-8 (browsers render them fine in
/// HTML5 `<meta charset="utf-8">` context).
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_plan() -> VerifyHtmlPlan {
        VerifyHtmlPlan {
            dataset_name: "my-org/my-dataset".to_string(),
            certificate_identity:
                "https://github.com/my-org/my-dataset/.github/workflows/build.yml@refs/heads/main"
                    .to_string(),
            certificate_oidc_issuer: "https://token.actions.githubusercontent.com".to_string(),
            bundle_path_in_repo: "attestrum/bundle.sigstore.json".to_string(),
            manifest_path_in_repo: "attestrum/manifest.parquet".to_string(),
        }
    }

    #[test]
    fn render_contains_dataset_name_and_identity_and_cosign_command() {
        let html = render(&fixture_plan()).expect("render");
        assert!(html.contains("Verify my-org/my-dataset"));
        assert!(html.contains(
            "https://github.com/my-org/my-dataset/.github/workflows/build.yml@refs/heads/main"
        ));
        assert!(html.contains("https://token.actions.githubusercontent.com"));
        assert!(html.contains("cosign verify-blob-attestation"));
        assert!(html.contains("--new-bundle-format"));
        assert!(html.contains("attestrum/bundle.sigstore.json"));
        assert!(html.contains("attestrum/manifest.parquet"));
    }

    #[test]
    fn render_escapes_html_injection_in_san() {
        let mut plan = fixture_plan();
        plan.certificate_identity = "<script>alert('xss')</script>".to_string();
        let html = render(&plan).expect("render");
        assert!(
            !html.contains("<script>alert('xss')</script>"),
            "raw injection must not appear: {html}"
        );
        assert!(
            html.contains("&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"),
            "escaped form must appear: {html}"
        );
    }

    #[test]
    fn render_escapes_ampersand_and_quotes_in_oidc_issuer() {
        let mut plan = fixture_plan();
        plan.certificate_oidc_issuer = r#"https://x.example?a=b&c="d""#.to_string();
        let html = render(&plan).expect("render");
        assert!(html.contains("a=b&amp;c=&quot;d&quot;"), "got: {html}");
    }

    #[test]
    fn render_is_deterministic_across_calls() {
        let a = render(&fixture_plan()).expect("render a");
        let b = render(&fixture_plan()).expect("render b");
        assert_eq!(a, b);
    }

    #[test]
    fn render_rejects_empty_dataset_name() {
        let mut plan = fixture_plan();
        plan.dataset_name = String::new();
        let err = render(&plan).expect_err("empty dataset_name must error");
        assert!(matches!(err, AttestrumEmitError::VerifyHtml(_)));
    }

    #[test]
    fn render_rejects_empty_certificate_identity() {
        let mut plan = fixture_plan();
        plan.certificate_identity = String::new();
        let err = render(&plan).expect_err("empty certificate_identity must error");
        assert!(matches!(err, AttestrumEmitError::VerifyHtml(_)));
    }

    #[test]
    fn render_rejects_empty_certificate_oidc_issuer() {
        let mut plan = fixture_plan();
        plan.certificate_oidc_issuer = String::new();
        let err = render(&plan).expect_err("empty certificate_oidc_issuer must error");
        assert!(matches!(err, AttestrumEmitError::VerifyHtml(_)));
    }

    #[test]
    fn render_rejects_empty_bundle_path() {
        let mut plan = fixture_plan();
        plan.bundle_path_in_repo = String::new();
        let err = render(&plan).expect_err("empty bundle_path_in_repo must error");
        assert!(matches!(err, AttestrumEmitError::VerifyHtml(_)));
    }

    #[test]
    fn render_rejects_empty_manifest_path() {
        let mut plan = fixture_plan();
        plan.manifest_path_in_repo = String::new();
        let err = render(&plan).expect_err("empty manifest_path_in_repo must error");
        assert!(matches!(err, AttestrumEmitError::VerifyHtml(_)));
    }

    #[test]
    fn html_escape_covers_five_owasp_characters() {
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("a<b"), "a&lt;b");
        assert_eq!(html_escape("a>b"), "a&gt;b");
        assert_eq!(html_escape("a\"b"), "a&quot;b");
        assert_eq!(html_escape("a'b"), "a&#x27;b");
    }

    #[test]
    fn html_escape_passes_non_ascii_through() {
        // UTF-8 chars are rendered fine in HTML5 with <meta charset="utf-8">.
        assert_eq!(html_escape("café—λ"), "café—λ");
    }
}
