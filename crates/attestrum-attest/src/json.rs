//! Deterministic JSON serialization helpers.
//!
//! All attestrum-attest output that gets signed, hashed, or persisted MUST go
//! through one of these helpers. They wrap `serde_json` with recursive
//! object-key sorting so two runs over the same input produce byte-identical
//! output — the foundation of the cross-target byte-identity gate enforced
//! by CLAUDE.md §7 and `.github/workflows/determinism.yml`.
//!
//! Use [`deterministic_json`] / [`deterministic_json_vec`] for the common
//! sort-then-serialize case. Use [`sort_keys`] directly if you need a custom
//! serialization shape (e.g., pretty-printed schema files in
//! `tests/schema_derive.rs`'s `derive_canonical_schema_json`).
//!
//! **Replaces** (Sprint 4 E3.6) three duplicate hand-rolled recursive-sort
//! functions that previously existed in `statement.rs` (`sort_value_keys`),
//! `canonicalize.rs` (`sort_keys_recursively`), and `tests/schema_derive.rs`
//! (`sort_keys_recursively`). The duplicates were correct but the discipline
//! lived in human memory — any new serialization callsite had to remember to
//! sort first. This module centralizes the discipline.
//!
//! **What's intentionally NOT here**: number-form canonicalization, string-
//! escape normalization (full RFC 8785 JCS). For Attestrum's use case we control
//! both sides of every byte comparison — emitter and verifier are both
//! attestrum-attest. serde_json's default number and string formatting is stable
//! across versions on the same machine. If we ever byte-compare against
//! externally-emitted bundles we'd need to upgrade to full JCS.

use serde::Serialize;
use serde_json::Value;

/// Serialize `value` to a JSON string with all object keys recursively
/// sorted. The only sanctioned path from a `Serialize` type to bytes for
/// any output that's signed, hashed, or persisted inside attestrum-attest.
///
/// Round-trips through `serde_json::Value` so the same sort applies to
/// nested objects produced by `#[serde(flatten)]`, `#[serde(tag = "...")]`
/// enum tagging, and `serde_json::Value` predicate payloads.
pub fn deterministic_json<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    let sorted = sort_keys(v);
    serde_json::to_string(&sorted)
}

/// Byte-vec variant of [`deterministic_json`]. Same guarantees.
pub fn deterministic_json_vec<T: Serialize + ?Sized>(
    value: &T,
) -> Result<Vec<u8>, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    let sorted = sort_keys(v);
    serde_json::to_vec(&sorted)
}

/// Recursively sort all object keys in a `serde_json::Value` tree. Arrays
/// preserve element order (in-toto subjects + tlog entries are ordered per
/// spec; reordering would break verifier semantics). Exposed so callers
/// needing custom serialization (e.g., pretty-printed committed schemas)
/// can still get the sort-once-serialize-later separation.
pub fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                sorted.insert(k, sort_keys(v));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deterministic_json_sorts_top_level_object_keys() {
        let v = json!({ "z": 1, "a": 2, "m": 3 });
        let out = deterministic_json(&v).unwrap();
        assert_eq!(out, r#"{"a":2,"m":3,"z":1}"#);
    }

    #[test]
    fn deterministic_json_sorts_nested_object_keys() {
        let v = json!({ "outer": { "z": 1, "a": 2 } });
        let out = deterministic_json(&v).unwrap();
        assert_eq!(out, r#"{"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn deterministic_json_preserves_array_order() {
        let v = json!({ "items": [3, 1, 2] });
        let out = deterministic_json(&v).unwrap();
        assert_eq!(out, r#"{"items":[3,1,2]}"#);
    }

    #[test]
    fn deterministic_json_sorts_objects_inside_arrays() {
        let v = json!({ "items": [{ "z": 1, "a": 2 }, { "y": 3, "b": 4 }] });
        let out = deterministic_json(&v).unwrap();
        assert_eq!(out, r#"{"items":[{"a":2,"z":1},{"b":4,"y":3}]}"#);
    }

    #[test]
    fn deterministic_json_is_byte_identical_across_repeated_calls() {
        let v = json!({ "z": [3, 1, 2], "a": { "nested": true, "deep": { "y": 1, "x": 2 } } });
        let a = deterministic_json(&v).unwrap();
        let b = deterministic_json(&v).unwrap();
        assert_eq!(
            a, b,
            "two calls over identical input must produce identical bytes"
        );
    }

    #[test]
    fn deterministic_json_vec_matches_deterministic_json_bytes() {
        let v = json!({ "z": 1, "a": 2 });
        let s = deterministic_json(&v).unwrap();
        let bytes = deterministic_json_vec(&v).unwrap();
        assert_eq!(s.as_bytes(), bytes.as_slice());
    }

    #[test]
    fn sort_keys_is_idempotent() {
        let v = json!({ "z": 1, "a": 2 });
        let once = sort_keys(v);
        let twice = sort_keys(once.clone());
        assert_eq!(once, twice);
    }

    #[test]
    fn deterministic_json_handles_serializable_struct() {
        #[derive(Serialize)]
        struct Inner {
            z: u32,
            a: u32,
        }
        #[derive(Serialize)]
        struct Outer {
            inner: Inner,
            tag: String,
        }
        // Field-declaration order in serde puts `tag` after `inner`, and
        // `z` before `a`. The sort flips both: `inner` < `tag` (already
        // sorted) and `a` < `z` (needs flip).
        let v = Outer {
            inner: Inner { z: 1, a: 2 },
            tag: "hi".to_string(),
        };
        let out = deterministic_json(&v).unwrap();
        assert_eq!(out, r#"{"inner":{"a":2,"z":1},"tag":"hi"}"#);
    }
}
