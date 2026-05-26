//! JSON Schema derivation + golden-file comparison for `FingerprintBundle`.
//!
//! Mirrors `crates/attestrum-attest/tests/schema_derive.rs` precedent
//! (Sprint 4 E2.5). The JSON Schema published at
//! `https://attestrum.com/fingerprint/v0.1.schema.json` is derived from
//! `FingerprintBundle` (via the `schemars::JsonSchema` derive that lands
//! at Sprint 5 S5-D1 E5) and pinned in `docs/schemas/fingerprint-v0.1.schema.json`.
//! Any drift between the Rust types and the published schema is a CI
//! break — fix one or the other in the same commit per CLAUDE.md §2.
//!
//! Regen the schema file via `ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p
//! attestrum-fingerprint --test schema_derive`. Same env var name as the
//! attestrum-attest precedent so a single regen invocation across all
//! `--test schema_derive` targets updates every committed schema.
//!
//! **PROTECTED note**: the `FINGERPRINT_SCHEMA` URI const + the schema
//! shape derived here are PROTECTED per CLAUDE.md §4 (any inclusion proof
//! that cites `attestrum.com/fingerprint/v0.1` references this exact
//! schema). Any change to the derived schema shape requires either a
//! v0.1 URI bump (with the PROTECTED footer + migration doc) OR a
//! backward-compatible v0.1 amendment (PROTECTED footer + a note that
//! v0.1 validators continue to accept the new shape).

use std::env;
use std::fs;
use std::path::PathBuf;

use attestrum_fingerprint::{FingerprintBundle, FINGERPRINT_SCHEMA};
use schemars::schema_for;
use serde_json::json;

/// Workspace-root-relative output dir for the published schema files.
const SCHEMAS_DIR_FROM_WORKSPACE_ROOT: &str = "docs/schemas";

/// Returns `<workspace-root>/docs/schemas`.
fn schemas_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR for attestrum-fingerprint = <ws>/crates/attestrum-fingerprint.
    // Two `parent()` calls reach the workspace root, then descend.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root from crates/attestrum-fingerprint");
    workspace_root.join(SCHEMAS_DIR_FROM_WORKSPACE_ROOT)
}

/// Recursively sort the keys of every JSON object so the pretty-printed
/// output is byte-stable across runs. schemars 1.x emits objects via
/// `serde_json::Map` whose iteration order depends on the `preserve_order`
/// feature; sorting defensively here matches the attestrum-attest
/// precedent and gives a single canonical byte form for the committed
/// golden.
fn sort_keys(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, Value> =
                std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, sort_keys(v));
            }
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Derive a schema for `FingerprintBundle`, inject the spec-mandated
/// `$id` + `title`, sort recursively, pretty-format with trailing newline.
fn derive_canonical_schema_json() -> String {
    let schema = schema_for!(FingerprintBundle);
    let mut value = serde_json::to_value(&schema).expect("schema -> value");
    if let Some(obj) = value.as_object_mut() {
        obj.insert("$id".to_string(), json!(schema_id_url()));
        obj.insert(
            "title".to_string(),
            json!("Attestrum Fingerprint Bundle v0.1"),
        );
    }
    let sorted = sort_keys(value);
    let mut text = serde_json::to_string_pretty(&sorted).expect("value -> pretty string");
    text.push('\n');
    text
}

/// Build the `.schema.json` URL from the `FINGERPRINT_SCHEMA` URI const.
/// `https://attestrum.com/fingerprint/v0.1` → `…/v0.1.schema.json`.
fn schema_id_url() -> String {
    format!("{FINGERPRINT_SCHEMA}.schema.json")
}

fn check_or_regen(file_name: &str, derived: &str) {
    let path = schemas_dir().join(file_name);

    if env::var("ATTESTRUM_REGEN_SCHEMAS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("create {}: {e}", parent.display()));
        }
        fs::write(&path, derived).unwrap_or_else(|e| panic!("regen write {}: {e}", path.display()));
        eprintln!("regenerated {}", path.display());
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read schema {}: {e}\nHint: ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p attestrum-fingerprint --test schema_derive",
            path.display()
        )
    });

    if derived != expected {
        let diff = format!(
            "Derived FingerprintBundle schema differs from committed {}.\n\n  Derived (first 500 chars):\n{}\n\n  Committed (first 500 chars):\n{}\n\nIf this drift is intentional and v0.1-compatible: regen via\n  ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p attestrum-fingerprint --test schema_derive\nand include the schema delta in the commit body with the\n`Protected-system-change: approved-by=...` footer per CLAUDE.md §4.\n",
            path.display(),
            derived.chars().take(500).collect::<String>(),
            expected.chars().take(500).collect::<String>(),
        );
        panic!("{diff}");
    }
}

#[test]
fn fingerprint_bundle_schema_matches_committed() {
    let derived = derive_canonical_schema_json();
    check_or_regen("fingerprint-v0.1.schema.json", &derived);
}

#[test]
fn derived_schema_is_byte_identical_across_repeated_derivations() {
    // Determinism guard: two derivations of FingerprintBundle must
    // produce byte-identical canonical JSON. Without this the schema
    // file would drift across rebuilds and CI would be unreliable.
    let a = derive_canonical_schema_json();
    let b = derive_canonical_schema_json();
    assert_eq!(a, b);
}

#[test]
fn schema_id_matches_published_uri_plus_schema_dot_json() {
    assert_eq!(
        schema_id_url(),
        "https://attestrum.com/fingerprint/v0.1.schema.json"
    );
}
