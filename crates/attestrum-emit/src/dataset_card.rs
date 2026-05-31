//! Hugging Face dataset card README.md emitter.
//!
//! Spec: PATH-A-BRIEF §2.3 "Dataset card YAML frontmatter (generated)" +
//! roadmap §E5 "YAML frontmatter + provenance section" addendum.
//!
//! Hand-rolled YAML emission (no `serde_yaml` dep per the E1 Cargo.toml
//! comment "Deferred: no template engine ... to keep deps minimal and output
//! byte-deterministic"). Output is byte-deterministic by construction: every
//! key/value pair is written in fixed order, sequences are emitted as
//! flow-style `["a", "b", "c"]` (the `tags:` block uses block-style for
//! readability), the `attestrum:` block uses block-style key:value with
//! alphabetically-sorted keys.
//!
//! The Hub passes through unknown extension keys (the `attestrum:` block is
//! non-Hub-reserved); the verify.html stub at E6 reads from it. The
//! `configs:` block from the brief is omitted at v0.1 per S5-D3 E5 plan-mode
//! 2026-05-28 founder decision — the brief's hard-coded `data/*.parquet`
//! shape is wrong for non-Parquet adopters and the Hub treats it as optional.
//!
//! Five `attestrum:` extension keys locked at E5:
//!   - predicate    → https://attestrum.com/attestation/training-corpus/v0.3
//!   - manifest     → attestrum/manifest.parquet
//!   - merkle_root  → attestrum/merkle.root
//!   - bundle       → attestrum/bundle.sigstore.json
//!   - verify_url   → ./attestrum/verify.html

use crate::{AttestrumEmitError, DatasetCardPlan};
use attestrum_attest::TRAINING_CORPUS_PREDICATE_TYPE;
use std::fmt::Write;

// Predicate URI is sourced from the canonical const in `attestrum-attest`
// (`TRAINING_CORPUS_PREDICATE_TYPE`) — single source of truth, no drift.
const ATTESTRUM_MANIFEST_PATH: &str = "attestrum/manifest.parquet";
const ATTESTRUM_MERKLE_ROOT_PATH: &str = "attestrum/merkle.root";
const ATTESTRUM_BUNDLE_PATH: &str = "attestrum/bundle.sigstore.json";
const ATTESTRUM_VERIFY_HTML_PATH: &str = "./attestrum/verify.html";

/// Required Hub tags. Always emitted regardless of caller-supplied tags;
/// appended AFTER the caller's tags so caller's first-tag stays first.
const REQUIRED_TAGS: [&str; 3] = ["attestrum-provenance", "sigstore-signed", "croissant"];

/// Render the dataset card README. See module docs for the determinism
/// contract, the five `attestrum:` extension keys, and the brief-vs-shipped
/// schema deltas (omitted `configs:` + `publication_intent:`).
pub fn render(plan: &DatasetCardPlan) -> Result<String, AttestrumEmitError> {
    if plan.pretty_name.is_empty() {
        return Err(AttestrumEmitError::Readme(
            "pretty_name is required".to_string(),
        ));
    }
    if plan.license_spdx.is_empty() {
        return Err(AttestrumEmitError::Readme(
            "license_spdx is required (use \"mixed\" for multi-license)".to_string(),
        ));
    }

    let mut out = String::with_capacity(1024);

    // YAML frontmatter — opening delimiter.
    out.push_str("---\n");

    // Top-level keys in fixed alphabetical order (byte-determinism without
    // depending on plan-field declaration order).
    writeln!(out, "dataset_name: {:?}", plan.dataset_name).unwrap();
    writeln!(out, "language: {}", yaml_flow_seq(&plan.language)).unwrap();
    // HF Hub's dataset-card validator rejects YAML frontmatter `license:`
    // values that aren't in its (lowercase) controlled vocabulary —
    // SPDX-canonical capitalization like `Apache-2.0` returns HTTP 400 at
    // commit time. The 2026-05-28 smoke test against `Attestrum/smoke-test`
    // caught this. The HF vocab IS the lowercased SPDX identifier for
    // common licenses (apache-2.0, mit, bsd-3-clause, cc-by-4.0, …), so a
    // simple `to_lowercase()` covers the canonical SPDX inputs without a
    // lookup table; exotic values that don't match HF's allowlist still
    // surface as a Hub-side 400 (with a useful error message) for the
    // caller to remap to `other` themselves.
    let license_for_hub = plan.license_spdx.to_lowercase();
    writeln!(out, "license: {license_for_hub:?}").unwrap();
    if plan.license_spdx == "mixed" {
        out.push_str("license_details: \"see attestrum/license-inventory.json\"\n");
    }
    writeln!(out, "pretty_name: {:?}", plan.pretty_name).unwrap();
    writeln!(out, "size_categories: [{:?}]", plan.size_category).unwrap();
    writeln!(
        out,
        "task_categories: {}",
        yaml_flow_seq(&plan.task_categories)
    )
    .unwrap();

    // tags: block-style; caller-supplied first, then the three required.
    out.push_str("tags:\n");
    for t in &plan.tags {
        writeln!(out, "  - {t:?}").unwrap();
    }
    for t in REQUIRED_TAGS {
        writeln!(out, "  - {t:?}").unwrap();
    }

    // attestrum: extension block — alphabetically sorted keys.
    out.push_str("attestrum:\n");
    writeln!(out, "  bundle: {ATTESTRUM_BUNDLE_PATH:?}").unwrap();
    writeln!(out, "  manifest: {ATTESTRUM_MANIFEST_PATH:?}").unwrap();
    writeln!(out, "  merkle_root: {ATTESTRUM_MERKLE_ROOT_PATH:?}").unwrap();
    writeln!(out, "  predicate: {TRAINING_CORPUS_PREDICATE_TYPE:?}").unwrap();
    writeln!(out, "  verify_url: {ATTESTRUM_VERIFY_HTML_PATH:?}").unwrap();

    // YAML frontmatter — closing delimiter.
    out.push_str("---\n\n");

    // Markdown body — canned text with templated values, no nondeterministic ordering.
    writeln!(out, "# {}\n", plan.pretty_name).unwrap();
    out.push_str(
        "This dataset's provenance is cryptographically verifiable. The corpus's \
         training-time content is described by a sealed Merkle-rooted manifest signed \
         with Sigstore. The signing identity is recorded in a Rekor transparency-log \
         entry; anyone can verify the chain end-to-end without Attestrum installed.\n\n",
    );

    out.push_str("## Verification\n\n");
    writeln!(
        out,
        "- Hosted verify page: [{}]({})",
        plan.verify_url, plan.verify_url
    )
    .unwrap();
    writeln!(
        out,
        "- CLI: `cosign verify-blob-attestation --new-bundle-format \
         --type {TRAINING_CORPUS_PREDICATE_TYPE} \
         --bundle attestrum/bundle.sigstore.json attestrum/manifest.parquet`\n"
    )
    .unwrap();

    out.push_str("## Corpus stats\n\n");
    writeln!(out, "- Documents: {}", plan.manifest_stats.leaf_count).unwrap();
    writeln!(out, "- Total bytes: {}\n", plan.manifest_stats.total_bytes).unwrap();

    // Optional source/attribution section, rendered verbatim from the
    // caller-supplied markdown. The emitter authors no attribution text — the
    // publisher owns the license-required credit / source / modification /
    // ShareAlike content. `trim_end()` + a fixed `\n\n` keeps the section
    // byte-deterministic regardless of the supplied string's trailing whitespace.
    if let Some(attribution) = &plan.attribution {
        out.push_str("## Source & attribution\n\n");
        out.push_str(attribution.trim_end());
        out.push_str("\n\n");
    }

    out.push_str("## Attestrum metadata\n\n");
    writeln!(
        out,
        "The provenance descriptor (Croissant JSON-LD) lives at `croissant.json`; \
         the sealed manifest at `attestrum/manifest.parquet`; the Merkle root at \
         `attestrum/merkle.root`; the Sigstore bundle at \
         `attestrum/bundle.sigstore.json`. The signing predicate is \
         `{TRAINING_CORPUS_PREDICATE_TYPE}`."
    )
    .unwrap();

    Ok(out)
}

/// Render a `&[String]` as a YAML flow-sequence: `["a", "b", "c"]`. Empty
/// slice renders as `[]`. Each element is YAML-double-quoted via Debug
/// formatting (which handles backslash + double-quote escaping for ASCII).
fn yaml_flow_seq(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let inner: Vec<String> = items.iter().map(|s| format!("{s:?}")).collect();
    format!("[{}]", inner.join(", "))
}
