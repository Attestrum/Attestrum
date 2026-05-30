//! JSON Schema derivation + golden-file comparison.
//!
//! Per `docs/cross-checks/e1.5/resolution.md` §7 + `docs/diagrams/sprint-4/
//! predicate-three-types.md` schema-derivation invariant: the JSON Schema
//! files published at the three `https://attestrum.com/.../v0.3.schema.json`
//! URLs are derived from the predicate Rust types in `crates/attestrum-attest/
//! src/predicate.rs` via the `schemars` crate. The locally-committed copies
//! live at `docs/schemas/*.schema.json` and ARE the bytes that would be
//! served at the URLs once the `attestrum.com` domain hosts them.
//!
//! This test re-derives each schema from the current Rust types and diffs
//! against the checked-in golden files. Any drift between the Rust types
//! and the published schemas is a CI break — fix one or the other in the
//! same commit per CLAUDE.md §2.
//!
//! Regen the schema files via `ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p
//! attestrum-attest --test schema_derive`. Mirrors the standard
//! `INSTA_UPDATE=1` / `ATTESTRUM_REGEN_GOLDEN=1` / `ATTESTRUM_REGEN_API_SURFACE=1`
//! conventions established by prior commits.
//!
//! **PROTECTED note**: schemars-derived schemas at v0.3 are PROTECTED per
//! CLAUDE.md §4 (the URI strings published at E2 reference these schemas).
//! Any change to the derived schema shape requires either a v0.3 URI bump
//! (with the PROTECTED footer + migration doc) OR a backward-compatible
//! v0.3 amendment (with the PROTECTED footer + a note that v0.3 validators
//! continue to accept the new shape).

use std::env;
use std::fs;
use std::path::PathBuf;

use attestrum_attest::{
    InclusionProofPredicate, ModelBindingPredicate, NonInclusionProofPredicate,
    TrainingCorpusPredicate, INCLUSION_PROOF_PREDICATE_TYPE, MODEL_BINDING_PREDICATE_TYPE,
    NON_INCLUSION_PROOF_PREDICATE_TYPE, TRAINING_CORPUS_PREDICATE_TYPE,
};
use schemars::schema_for;
use serde_json::json;

/// Workspace-root-relative output dir for the published schema files.
const SCHEMAS_DIR_FROM_WORKSPACE_ROOT: &str = "docs/schemas";

/// Returns `<workspace-root>/docs/schemas`.
fn schemas_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR for attestrum-attest = <ws>/crates/attestrum-attest.
    // Go up two parents to reach <ws>, then descend into docs/schemas.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root from crates/attestrum-attest");
    workspace_root.join(SCHEMAS_DIR_FROM_WORKSPACE_ROOT)
}

/// Derive a schema for `T`, inject the spec-mandated `$id` (the
/// `.schema.json` URL corresponding to the predicate's URI string) and
/// override the schemars-default `title` (which is the Rust type name) to
/// the PATH-A-BRIEF §3.1/§3.2/§3.3 human-facing title. Format as canonical
/// pretty JSON (recursive key sort, 2-space indent, trailing newline).
/// The pretty-print is for human review of the committed schema files;
/// byte-identity across runs is guaranteed by `serde_json::to_string_pretty`
/// over a pre-sorted Value.
fn derive_canonical_schema_json<T: schemars::JsonSchema>(id_url: &str, title: &str) -> String {
    let schema = schema_for!(T);
    let mut value = serde_json::to_value(&schema).expect("schema -> value");
    if let Some(obj) = value.as_object_mut() {
        obj.insert("$id".to_string(), json!(id_url));
        obj.insert("title".to_string(), json!(title));
    }
    let sorted = attestrum_attest::sort_keys(value);
    let mut text = serde_json::to_string_pretty(&sorted).expect("value -> pretty string");
    text.push('\n');
    text
}

/// Build the `.schema.json` URL from a predicate-type URI.
/// `https://attestrum.com/attestation/foo/v0.3` → `https://attestrum.com/attestation/foo/v0.3.schema.json`
fn schema_id_url(predicate_type_uri: &str) -> String {
    format!("{predicate_type_uri}.schema.json")
}

fn check_or_regen(file_name: &str, derived: &str) {
    let path = schemas_dir().join(file_name);

    if env::var("ATTESTRUM_REGEN_SCHEMAS").is_ok() {
        // Ensure the docs/schemas/ dir exists when regenerating.
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
            "read schema {}: {e}\nHint: ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p attestrum-attest --test schema_derive",
            path.display()
        )
    });

    if derived != expected {
        let diff = format!(
            "Derived schema differs from committed {}.\n\n  Derived (first 500 chars):\n{}\n\n  Committed (first 500 chars):\n{}\n\nIf this drift is intentional and v0.3-compatible: regen via\n  ATTESTRUM_REGEN_SCHEMAS=1 cargo test -p attestrum-attest --test schema_derive\nand include the schema delta in the commit body with the\n`Protected-system-change: approved-by=...` footer per CLAUDE.md §4.\n",
            path.display(),
            derived.chars().take(500).collect::<String>(),
            expected.chars().take(500).collect::<String>(),
        );
        panic!("{diff}");
    }
}

#[test]
fn training_corpus_schema_matches_committed() {
    let derived = derive_canonical_schema_json::<TrainingCorpusPredicate>(
        &schema_id_url(TRAINING_CORPUS_PREDICATE_TYPE),
        "Attestrum Training Corpus Attestation v0.3",
    );
    check_or_regen("training-corpus-v0.3.schema.json", &derived);
}

#[test]
fn inclusion_proof_schema_matches_committed() {
    let derived = derive_canonical_schema_json::<InclusionProofPredicate>(
        &schema_id_url(INCLUSION_PROOF_PREDICATE_TYPE),
        "Attestrum Inclusion Proof v0.3",
    );
    check_or_regen("inclusion-proof-v0.3.schema.json", &derived);
}

#[test]
fn non_inclusion_proof_schema_matches_committed() {
    let derived = derive_canonical_schema_json::<NonInclusionProofPredicate>(
        &schema_id_url(NON_INCLUSION_PROOF_PREDICATE_TYPE),
        "Attestrum Non-Inclusion Proof v0.3",
    );
    check_or_regen("non-inclusion-proof-v0.3.schema.json", &derived);
}

#[test]
fn model_binding_schema_matches_committed() {
    let derived = derive_canonical_schema_json::<ModelBindingPredicate>(
        &schema_id_url(MODEL_BINDING_PREDICATE_TYPE),
        "Attestrum Model Binding v0.1",
    );
    check_or_regen("model-binding-v0.1.schema.json", &derived);
}

#[test]
fn derived_schema_is_byte_identical_across_repeated_derivations() {
    // Determinism guard: two derivations of the same type must produce
    // byte-identical canonical JSON. Without this, the schema files would
    // drift across rebuilds and CI would be unreliable.
    let a = derive_canonical_schema_json::<TrainingCorpusPredicate>(
        &schema_id_url(TRAINING_CORPUS_PREDICATE_TYPE),
        "Attestrum Training Corpus Attestation v0.3",
    );
    let b = derive_canonical_schema_json::<TrainingCorpusPredicate>(
        &schema_id_url(TRAINING_CORPUS_PREDICATE_TYPE),
        "Attestrum Training Corpus Attestation v0.3",
    );
    assert_eq!(a, b);
}

#[test]
fn schema_id_matches_published_predicate_type_uri_plus_schema_dot_json() {
    assert_eq!(
        schema_id_url(TRAINING_CORPUS_PREDICATE_TYPE),
        "https://attestrum.com/attestation/training-corpus/v0.3.schema.json"
    );
    assert_eq!(
        schema_id_url(INCLUSION_PROOF_PREDICATE_TYPE),
        "https://attestrum.com/attestation/inclusion-proof/v0.3.schema.json"
    );
    assert_eq!(
        schema_id_url(NON_INCLUSION_PROOF_PREDICATE_TYPE),
        "https://attestrum.com/attestation/non-inclusion-proof/v0.3.schema.json"
    );
}
