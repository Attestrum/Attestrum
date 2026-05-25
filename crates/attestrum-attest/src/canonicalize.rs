//! Sigstore Bundle v0.3 strip-set + canonical-JSON helper for cross-platform
//! byte-determinism comparison.
//!
//! The CI determinism matrix (4 targets: linux-x86_64-glibc, linux-aarch64-
//! glibc, macos-aarch64-darwin, linux-x86_64-musl) pairwise-`cmp`s bundles
//! emitted from identical input. Bundle v0.3 contains 16 fields that are
//! legitimately non-deterministic across runs (ephemeral keypairs, RFC3161
//! timestamps, Rekor tlog state, etc. — see
//! `docs/diagrams/sprint-4/verify-flow.md` strip-set table for the full list
//! and `docs/cross-checks/e1.5/resolution.md` §6.4 for the rationale).
//!
//! [`canonicalize_for_compare`] takes a bundle JSON `Value`, replaces each
//! of the 16 paths with a fixed sentinel (`"__ATTESTRUM_STRIPPED__"` for
//! string/base64 fields, `null` for object/array fields), recursively sorts
//! all object keys, and returns the canonical-JSON bytes ready for
//! pairwise `cmp`.
//!
//! **Invariant**: the canonical bundle is NOT verifiable by cosign (its
//! signatures + cert material are zeroed). It exists ONLY as a CI byte-
//! comparison artifact. The unmodified bundle is what gets shipped, what
//! gets verified by `cosign verify-blob-attestation`, and what verifiers
//! consume in the wild.
//!
//! **JCS-lite, not full RFC 8785**: this implementation does recursive
//! object-key sort + serde_json's standard number/string formatting. Full
//! RFC 8785 JCS would also normalize Unicode escapes + number canonical
//! form. For our use case (we control both sides of the comparison —
//! Attestrum-emitted bundles only) the lite form is sufficient. Upgrade to
//! full JCS if we ever need to byte-compare against external Sigstore-
//! emitting tools.

use serde_json::Value;

use crate::AttestrumAttestError;

/// Fixed sentinel for stripped string/base64 values. Chosen per
/// `docs/cross-checks/e1.5/resolution.md` §6.4 to be clearly NOT a real
/// hash, signature, or timestamp — a reviewer reading the canonical
/// bundle won't mistake it for a real value (vs. `"0000..."` which could).
pub const STRIP_SENTINEL: &str = "__ATTESTRUM_STRIPPED__";

/// The 16 ordered Bundle v0.3 paths stripped before pairwise byte-cmp.
/// Each path is a sequence of [`PathSegment`]s navigating from the bundle
/// root to the leaf value being replaced.
///
/// Same content as the table in `docs/diagrams/sprint-4/verify-flow.md`;
/// keep the two in sync — if a Sigstore Bundle v0.3 wire-format change
/// adds a new non-deterministic field, add both a row to the diagram
/// table AND a path here in the same commit, with a `Protected-system-
/// change:` footer per CLAUDE.md §4.
pub const STRIP_PATHS: &[&[PathSegment]] = &[
    // 1. Keyless leaf cert DER blob (ephemeral pubkey + validity + serial + Fulcio sig).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("certificate"),
        PathSegment::Key("rawBytes"),
    ],
    // 2. Legacy chain-form cert DER blob (per-cert variant of #1).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("x509CertificateChain"),
        PathSegment::Key("certificates"),
        PathSegment::EachArrayElement,
        PathSegment::Key("rawBytes"),
    ],
    // 3. RFC3161 TSA timestamp (signed time + nonce-dependent bytes).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("timestampVerificationData"),
        PathSegment::Key("rfc3161Timestamps"),
        PathSegment::EachArrayElement,
        PathSegment::Key("signedTimestamp"),
    ],
    // 4. Rekor wall-clock integration time.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("integratedTime"),
    ],
    // 5. Rekor global log ingest order.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("logIndex"),
    ],
    // 6. Rekor SET (signed entry timestamp).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionPromise"),
        PathSegment::Key("signedEntryTimestamp"),
    ],
    // 7. Inclusion-proof tree index at proof time.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionProof"),
        PathSegment::Key("logIndex"),
    ],
    // 8. Rekor tree state at proof time.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionProof"),
        PathSegment::Key("rootHash"),
    ],
    // 9. Rekor tree size at proof time.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionProof"),
        PathSegment::Key("treeSize"),
    ],
    // 10. Sibling-path hashes (each element).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionProof"),
        PathSegment::Key("hashes"),
        PathSegment::EachArrayElement,
    ],
    // 11. Signed checkpoint envelope over log tree state.
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("inclusionProof"),
        PathSegment::Key("checkpoint"),
        PathSegment::Key("envelope"),
    ],
    // 12. Rekor body (embeds sig + cert material + serialization details).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("tlogEntries"),
        PathSegment::EachArrayElement,
        PathSegment::Key("canonicalizedBody"),
    ],
    // 13. DSSE signature itself (ephemeral keypair → different bytes per run).
    //     Bundle v0.3 mandates exactly one signature, so this targets [0].
    &[
        PathSegment::Key("dsseEnvelope"),
        PathSegment::Key("signatures"),
        PathSegment::EachArrayElement,
        PathSegment::Key("sig"),
    ],
    // 14. DSSE keyid (conditional — strip if populated from ephemeral key).
    &[
        PathSegment::Key("dsseEnvelope"),
        PathSegment::Key("signatures"),
        PathSegment::EachArrayElement,
        PathSegment::Key("keyid"),
    ],
    // 15. PublicKeyIdentifier hint (non-cert ephemeral flow).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("publicKeyIdentifier"),
        PathSegment::Key("hint"),
    ],
    // 16. Embedded ephemeral public key (non-cert flow, mutually exclusive with cert).
    &[
        PathSegment::Key("verificationMaterial"),
        PathSegment::Key("publicKey"),
        PathSegment::Key("rawBytes"),
    ],
];

/// One step in a strip path: either a fixed object key or "all elements of
/// this array".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSegment {
    /// Descend into the named object key.
    Key(&'static str),
    /// Descend into every element of the current array (each becomes a
    /// separate strip target).
    EachArrayElement,
}

/// Replace every leaf reachable via one of the 16 [`STRIP_PATHS`] with the
/// strip sentinel ([`STRIP_SENTINEL`] for strings, [`Value::Null`] for
/// object/array leaves), recursively sort all object keys, and return the
/// canonical-JSON bytes ready for pairwise `cmp`.
///
/// Paths that don't resolve in the input bundle (because the bundle uses
/// only the cert form vs. the public-key form, or because a particular
/// tlog entry has no `inclusionPromise`, etc.) are silently skipped — the
/// strip is best-effort defensive coverage, not strict-path-must-exist.
pub fn canonicalize_for_compare(bundle: Value) -> Result<Vec<u8>, AttestrumAttestError> {
    let mut stripped = bundle;
    for path in STRIP_PATHS {
        apply_strip(&mut stripped, path);
    }
    // Route the final sort + serialize through the single sanctioned
    // helper (Sprint 4 E3.6); the strip step above is the only thing
    // unique to this codepath.
    Ok(crate::json::deterministic_json_vec(&stripped)?)
}

/// Walk a single strip path, replacing each terminal value with the
/// appropriate sentinel.
fn apply_strip(value: &mut Value, path: &[PathSegment]) {
    apply_strip_at(value, path, 0);
}

fn apply_strip_at(value: &mut Value, path: &[PathSegment], idx: usize) {
    if idx == path.len() {
        // Terminal — replace this leaf with the sentinel form matching its type.
        *value = strip_sentinel_for(value);
        return;
    }
    match &path[idx] {
        PathSegment::Key(k) => {
            if let Value::Object(obj) = value {
                if let Some(child) = obj.get_mut(*k) {
                    apply_strip_at(child, path, idx + 1);
                }
            }
        }
        PathSegment::EachArrayElement => {
            if let Value::Array(arr) = value {
                for child in arr.iter_mut() {
                    apply_strip_at(child, path, idx + 1);
                }
            }
        }
    }
}

/// Return the strip sentinel matching `value`'s JSON type so structural
/// shape is preserved (arrays stay arrays, objects stay objects).
fn strip_sentinel_for(value: &Value) -> Value {
    match value {
        Value::String(_) => Value::String(STRIP_SENTINEL.to_string()),
        Value::Number(_) => Value::Number(0.into()),
        Value::Bool(_) => Value::Bool(false),
        Value::Null => Value::Null,
        Value::Array(_) => Value::Null,
        Value::Object(_) => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal-but-realistic Bundle v0.3 shape sufficient to exercise
    /// every strip path. Not a real verifiable bundle — just enough JSON
    /// to navigate the 16 paths.
    fn sample_bundle() -> Value {
        json!({
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
            "dsseEnvelope": {
                "payload": "BASE64_OF_INTOTO_STATEMENT",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [
                    { "sig": "PER_RUN_SIG_BYTES", "keyid": "PER_RUN_KEYID" }
                ]
            },
            "verificationMaterial": {
                "certificate": { "rawBytes": "PER_RUN_CERT_DER" },
                "x509CertificateChain": {
                    "certificates": [
                        { "rawBytes": "LEGACY_CERT_DER_0" },
                        { "rawBytes": "LEGACY_CERT_DER_1" }
                    ]
                },
                "publicKey": { "rawBytes": "EMBEDDED_PUBKEY" },
                "publicKeyIdentifier": { "hint": "PER_RUN_HINT" },
                "timestampVerificationData": {
                    "rfc3161Timestamps": [
                        { "signedTimestamp": "TSA_RESPONSE_BLOB" }
                    ]
                },
                "tlogEntries": [
                    {
                        "integratedTime": "1748113200",
                        "logIndex": "42",
                        "canonicalizedBody": "REKOR_BODY_BLOB",
                        "inclusionPromise": {
                            "signedEntryTimestamp": "SET_BLOB"
                        },
                        "inclusionProof": {
                            "logIndex": "42",
                            "rootHash": "REKOR_ROOT_AT_PROOF_TIME",
                            "treeSize": "12345",
                            "hashes": ["SIBLING_HASH_0", "SIBLING_HASH_1"],
                            "checkpoint": {
                                "envelope": "SIGNED_CHECKPOINT_ENVELOPE"
                            }
                        }
                    }
                ]
            }
        })
    }

    #[test]
    fn strip_replaces_each_of_the_16_paths_with_sentinel() {
        let out_bytes = canonicalize_for_compare(sample_bundle()).unwrap();
        let out: Value = serde_json::from_slice(&out_bytes).unwrap();

        // Spot-check every one of the 16 paths.
        assert_eq!(
            out["verificationMaterial"]["certificate"]["rawBytes"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["x509CertificateChain"]["certificates"][0]["rawBytes"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["x509CertificateChain"]["certificates"][1]["rawBytes"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["timestampVerificationData"]["rfc3161Timestamps"][0]
                ["signedTimestamp"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["integratedTime"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["logIndex"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionPromise"]
                ["signedEntryTimestamp"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["logIndex"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["rootHash"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["treeSize"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][0],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][1],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]
                ["envelope"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["canonicalizedBody"],
            STRIP_SENTINEL
        );
        assert_eq!(out["dsseEnvelope"]["signatures"][0]["sig"], STRIP_SENTINEL);
        assert_eq!(
            out["dsseEnvelope"]["signatures"][0]["keyid"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["publicKeyIdentifier"]["hint"],
            STRIP_SENTINEL
        );
        assert_eq!(
            out["verificationMaterial"]["publicKey"]["rawBytes"],
            STRIP_SENTINEL
        );
    }

    #[test]
    fn strip_preserves_deterministic_fields() {
        let out_bytes = canonicalize_for_compare(sample_bundle()).unwrap();
        let out: Value = serde_json::from_slice(&out_bytes).unwrap();
        // payload + payloadType + mediaType + array lengths preserved.
        assert_eq!(out["dsseEnvelope"]["payload"], "BASE64_OF_INTOTO_STATEMENT");
        assert_eq!(
            out["dsseEnvelope"]["payloadType"],
            "application/vnd.in-toto+json"
        );
        assert_eq!(
            out["mediaType"],
            "application/vnd.dev.sigstore.bundle.v0.3+json"
        );
        // Arrays keep their original length (sentinel-replace, not remove).
        assert_eq!(
            out["verificationMaterial"]["x509CertificateChain"]["certificates"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            out["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn strip_is_deterministic_across_repeated_calls() {
        let a = canonicalize_for_compare(sample_bundle()).unwrap();
        let b = canonicalize_for_compare(sample_bundle()).unwrap();
        assert_eq!(a, b, "canonicalize_for_compare must be byte-deterministic");
    }

    #[test]
    fn strip_is_byte_identical_when_only_non_deterministic_fields_differ() {
        // Two bundles with the same deterministic content but different
        // strip-set values should canonicalize to the same bytes.
        let bundle_a = json!({
            "dsseEnvelope": {
                "payload": "SAME_PAYLOAD",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [{ "sig": "RUN_A_SIG", "keyid": "RUN_A_KEYID" }]
            },
            "verificationMaterial": {
                "certificate": { "rawBytes": "RUN_A_CERT" },
                "tlogEntries": [{
                    "integratedTime": "1700000000",
                    "logIndex": "100"
                }]
            },
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"
        });
        let bundle_b = json!({
            "dsseEnvelope": {
                "payload": "SAME_PAYLOAD",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [{ "sig": "RUN_B_SIG", "keyid": "RUN_B_KEYID" }]
            },
            "verificationMaterial": {
                "certificate": { "rawBytes": "RUN_B_CERT" },
                "tlogEntries": [{
                    "integratedTime": "1800000000",
                    "logIndex": "200"
                }]
            },
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"
        });
        let canonical_a = canonicalize_for_compare(bundle_a).unwrap();
        let canonical_b = canonicalize_for_compare(bundle_b).unwrap();
        assert_eq!(
            canonical_a, canonical_b,
            "bundles differing only in strip-set fields must canonicalize identically"
        );
    }

    #[test]
    fn strip_diverges_when_payload_differs() {
        // Inverse of the test above: bundles with DIFFERENT deterministic
        // content (different payload bytes) must canonicalize differently
        // — otherwise the strip is hiding real non-determinism.
        let bundle_a = json!({
            "dsseEnvelope": {
                "payload": "PAYLOAD_A",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [{ "sig": "X", "keyid": "X" }]
            },
            "mediaType": "x"
        });
        let bundle_b = json!({
            "dsseEnvelope": {
                "payload": "PAYLOAD_B",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [{ "sig": "X", "keyid": "X" }]
            },
            "mediaType": "x"
        });
        let canonical_a = canonicalize_for_compare(bundle_a).unwrap();
        let canonical_b = canonicalize_for_compare(bundle_b).unwrap();
        assert_ne!(canonical_a, canonical_b);
    }

    #[test]
    fn strip_handles_missing_paths_silently() {
        // A bundle with only the cert flow (no publicKey form) should not
        // panic when strip paths for the publicKey form try to navigate
        // through missing keys.
        let cert_only_bundle = json!({
            "dsseEnvelope": {
                "payload": "PAYLOAD",
                "payloadType": "application/vnd.in-toto+json",
                "signatures": [{ "sig": "S", "keyid": "K" }]
            },
            "verificationMaterial": {
                "certificate": { "rawBytes": "CERT" }
            },
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"
        });
        let out_bytes = canonicalize_for_compare(cert_only_bundle).unwrap();
        let out: Value = serde_json::from_slice(&out_bytes).unwrap();
        // Cert was stripped.
        assert_eq!(
            out["verificationMaterial"]["certificate"]["rawBytes"],
            STRIP_SENTINEL
        );
        // publicKey was absent; should remain absent (not auto-created).
        assert!(out["verificationMaterial"].get("publicKey").is_none());
    }

    #[test]
    fn strip_path_count_matches_resolution_doc() {
        // Sanity: 16 paths total per docs/cross-checks/e1.5/resolution.md §6.4.
        assert_eq!(STRIP_PATHS.len(), 16);
    }
}
