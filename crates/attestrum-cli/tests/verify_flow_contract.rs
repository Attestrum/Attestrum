//! Contract tests for the `attestrum verify` lifecycle state machine.
//!
//! Sprint 4 commit E4. Closes the FOURTH `sequenceDiagram` contract-test
//! obligation per PATH-A-BRIEF §7.1 / CLAUDE.md §7.1 (first was Sprint 2
//! E2 for `signal-decision.md`'s stateDiagram, second was Sprint 3 E6
//! for `attestrum-inspect-lifecycle.md`'s stateDiagram, third was Sprint 4
//! E3.5 for `sign-flow.md`'s sequenceDiagram). The diagram lives at
//! `docs/diagrams/sprint-4/verify-flow.md` (flipped to `source_of_truth: code`
//! in this same commit).
//!
//! Mirrors the shape of `tests/sign_flow_contract.rs` — same four
//! proptest properties plus three end-to-end smokes that lock the
//! lifecycle ordering for the common failure modes (missing bundle,
//! missing manifest, malformed bundle bytes).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cli::commands::verify::{run as verify_run, Args as VerifyArgs};
use attestrum_cli::lifecycle::{
    verify_all_events, verify_all_non_terminal_states, verify_documented_transitions,
    verify_transition, ExitCode, VerifyEvent, VerifyState,
};
use proptest::prelude::*;

// ============================================================================
// CARGO_TARGET_TMPDIR scratch dirs (mirrors sign_flow_contract.rs)
// ============================================================================

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-e4-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

// ============================================================================
// proptest strategies
// ============================================================================

fn arb_event() -> impl Strategy<Value = VerifyEvent> {
    let events = verify_all_events();
    (0..events.len()).prop_map(move |i| events[i])
}

fn arb_state() -> impl Strategy<Value = VerifyState> {
    let states = verify_all_non_terminal_states();
    (0..states.len()).prop_map(move |i| states[i])
}

fn arb_documented_event() -> impl Strategy<Value = VerifyEvent> {
    let docs: Vec<VerifyEvent> = verify_documented_transitions()
        .iter()
        .map(|(_, e, _)| *e)
        .collect();
    let len = docs.len();
    (0..len).prop_map(move |i| docs[i])
}

fn is_documented(from: VerifyState, event: VerifyEvent) -> bool {
    verify_documented_transitions()
        .iter()
        .any(|(s, e, _)| *s == from && *e == event)
}

fn is_terminal_allowed(code: ExitCode) -> bool {
    matches!(
        code,
        ExitCode::Ok
            | ExitCode::RuntimeError
            | ExitCode::ArgsError
            | ExitCode::OfflineViolation
            | ExitCode::NetworkError
            | ExitCode::VerificationFailure
            | ExitCode::SchemaError
    )
}

// ============================================================================
// Property 1: every documented transition is reachable
// ============================================================================

#[test]
fn every_documented_transition_is_reachable() {
    for (from, event, expected_to) in verify_documented_transitions() {
        let actual = verify_transition(*from, *event);
        assert_eq!(
            actual, *expected_to,
            "documented transition ({from:?}, {event:?}) → expected {expected_to:?}, got {actual:?}"
        );
    }
}

// ============================================================================
// Properties 2-4: proptest-driven enumeration
// ============================================================================

proptest! {
    #[test]
    fn no_undocumented_transition_is_taken(
        state in arb_state(),
        event in arb_event(),
    ) {
        if !is_documented(state, event) {
            let result = verify_transition(state, event);
            prop_assert_eq!(
                result, state,
                "undocumented ({:?}, {:?}) must hold the source state; got {:?}",
                state, event, result,
            );
        }
    }

    /// Random walks of documented events from Invoked either terminate
    /// in a documented Exit OR stay within the documented non-terminal
    /// state set.
    #[test]
    fn every_path_terminates_in_a_known_exit_code(
        events in prop::collection::vec(arb_documented_event(), 1..32),
    ) {
        let mut state = VerifyState::Invoked;
        for event in events {
            state = verify_transition(state, event);
            if let VerifyState::Exit(code) = state {
                prop_assert!(
                    is_terminal_allowed(code),
                    "terminal Exit code must be in the allowed set: got {:?}",
                    code,
                );
                return Ok(());
            }
        }
        prop_assert!(
            verify_all_non_terminal_states().contains(&state),
            "non-terminal end state must be in the documented non-terminal set: got {:?}",
            state,
        );
    }

    /// Random walks of arbitrary events from Invoked never produce an
    /// exit code outside the allowed set.
    #[test]
    fn exit_codes_are_in_the_allowed_set(
        events in prop::collection::vec(arb_event(), 1..64),
    ) {
        let mut state = VerifyState::Invoked;
        for event in events {
            state = verify_transition(state, event);
            if let VerifyState::Exit(code) = state {
                prop_assert!(
                    is_terminal_allowed(code),
                    "exit code outside allowed set: got {:?}",
                    code,
                );
                return Ok(());
            }
        }
    }
}

// ============================================================================
// End-to-end smoke 5: missing bundle returns exit 2 before any I/O.
// ============================================================================

#[test]
fn missing_bundle_returns_exit_2() {
    let root = fresh_root("missing_bundle");
    let nonexistent_bundle = root.join("nope.sigstore.json");
    // Manifest must exist so we know the failure is the bundle.
    let manifest = root.join("manifest.parquet");
    fs::write(&manifest, b"stub manifest bytes").expect("write stub manifest");

    let code = verify_run(VerifyArgs {
        bundle: nonexistent_bundle,
        manifest,
        certificate_identity: ".*".to_string(),
        certificate_oidc_issuer: ".*".to_string(),
        offline: true,
        print_predicate: false,
    });

    assert_eq!(
        code, 2,
        "expected exit 2 (ArgsError, missing bundle); got {code}"
    );
}

// ============================================================================
// End-to-end smoke 6: missing manifest returns exit 2 before crypto verify.
// ============================================================================

#[test]
fn missing_manifest_returns_exit_2() {
    let root = fresh_root("missing_manifest");
    let bundle = root.join("bundle.sigstore.json");
    fs::write(&bundle, b"{}").expect("write stub bundle");
    let nonexistent_manifest = root.join("nope.parquet");

    let code = verify_run(VerifyArgs {
        bundle,
        manifest: nonexistent_manifest,
        certificate_identity: ".*".to_string(),
        certificate_oidc_issuer: ".*".to_string(),
        offline: true,
        print_predicate: false,
    });

    assert_eq!(
        code, 2,
        "expected exit 2 (ArgsError, missing manifest); got {code}"
    );
}

// ============================================================================
// End-to-end smoke 7: malformed bundle bytes exits with one of the
// documented bundle-load failure codes (1 RuntimeError, 6
// VerificationFailure). Locks the boundary between "bundle is garbage
// bytes" and "bundle is JSON but cert is malformed."
//
// Garbage bytes: serde_json::from_slice fails → BundleReadIoError →
// exit 1 per the lifecycle's documented edge.
//
// JSON-but-no-cert: identity extraction fails → IdentityExtractFailed →
// exit 6.
// ============================================================================

#[test]
fn malformed_bundle_bytes_returns_exit_1_or_6() {
    let root = fresh_root("malformed_bundle");
    let bundle = root.join("bundle.sigstore.json");
    // Garbage: not even valid JSON.
    fs::write(&bundle, b"not really a bundle file").expect("write garbage bundle");
    let manifest = root.join("manifest.parquet");
    fs::write(&manifest, b"stub manifest bytes").expect("write stub manifest");

    let code = verify_run(VerifyArgs {
        bundle,
        manifest,
        certificate_identity: ".*".to_string(),
        certificate_oidc_issuer: ".*".to_string(),
        offline: true,
        print_predicate: false,
    });

    assert!(
        code == 1 || code == 6,
        "expected exit 1 (RuntimeError, bundle read/JSON fail) or 6 (VerificationFailure, identity extract fail); got {code}"
    );
}

#[test]
fn json_bundle_without_cert_returns_exit_6() {
    let root = fresh_root("json_no_cert");
    let bundle = root.join("bundle.sigstore.json");
    // Valid JSON but no certificate / verificationMaterial — identity
    // extraction will fail.
    fs::write(
        &bundle,
        br#"{"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"}"#,
    )
    .expect("write json-no-cert bundle");
    let manifest = root.join("manifest.parquet");
    fs::write(&manifest, b"stub manifest bytes").expect("write stub manifest");

    let code = verify_run(VerifyArgs {
        bundle,
        manifest,
        certificate_identity: ".*".to_string(),
        certificate_oidc_issuer: ".*".to_string(),
        offline: true,
        print_predicate: false,
    });

    assert_eq!(
        code, 6,
        "expected exit 6 (VerificationFailure, identity extract fail); got {code}"
    );
}
