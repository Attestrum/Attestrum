//! Contract tests for the `attestrum sign` lifecycle state machine.
//!
//! Sprint 4 commit E3.5. Closes the THIRD `sequenceDiagram` contract-test
//! obligation per PATH-A-BRIEF §7.1 / CLAUDE.md §7.1: every
//! `sequenceDiagram` diagram must have a contract test verifying message
//! order, types, and error paths. The diagram lives at
//! `docs/diagrams/sprint-4/sign-flow.md` (flipped to `source_of_truth: code`
//! in this same commit).
//!
//! Mirrors the shape of `tests/inspect_proptest.rs` (Sprint 3 E6) — same
//! four properties (documented-transitions reachable, undocumented-holds,
//! paths-terminate-in-known-exit, exit-codes-in-allowed-set) — plus one
//! end-to-end smoke that drives `commands::sign::run` with `--offline` and
//! asserts the documented Exit 3 + no bundle written.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use attestrum_cli::commands::sign::{run as sign_run, Args as SignArgs};
use attestrum_cli::lifecycle::{
    sign_all_events, sign_all_non_terminal_states, sign_documented_transitions, sign_transition,
    ExitCode, SignEvent, SignState,
};
use proptest::prelude::*;

// ============================================================================
// CARGO_TARGET_TMPDIR scratch dirs (mirrors inspect_proptest.rs:41-53)
// ============================================================================

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-e3_5-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

// ============================================================================
// proptest strategies
// ============================================================================

fn arb_event() -> impl Strategy<Value = SignEvent> {
    let events = sign_all_events();
    (0..events.len()).prop_map(move |i| events[i])
}

fn arb_state() -> impl Strategy<Value = SignState> {
    let states = sign_all_non_terminal_states();
    (0..states.len()).prop_map(move |i| states[i])
}

fn arb_documented_event() -> impl Strategy<Value = SignEvent> {
    let docs: Vec<SignEvent> = sign_documented_transitions()
        .iter()
        .map(|(_, e, _)| *e)
        .collect();
    let len = docs.len();
    (0..len).prop_map(move |i| docs[i])
}

fn is_documented(from: SignState, event: SignEvent) -> bool {
    sign_documented_transitions()
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
            | ExitCode::IdentityError
            | ExitCode::NetworkError
            | ExitCode::SchemaError
    )
}

// ============================================================================
// Property 1: every documented transition is reachable
//
// Exhaustive — locks the 22-edge spec from the diagram against the
// `sign_transition` function so a divergence between code and diagram
// shows up here immediately.
// ============================================================================

#[test]
fn every_documented_transition_is_reachable() {
    for (from, event, expected_to) in sign_documented_transitions() {
        let actual = sign_transition(*from, *event);
        assert_eq!(
            actual, *expected_to,
            "documented transition ({from:?}, {event:?}) → expected {expected_to:?}, got {actual:?}"
        );
    }
}

// ============================================================================
// Property 2: no undocumented transition is taken (no silent forward progress)
// ============================================================================

proptest! {
    #[test]
    fn no_undocumented_transition_is_taken(
        state in arb_state(),
        event in arb_event(),
    ) {
        if !is_documented(state, event) {
            let result = sign_transition(state, event);
            prop_assert_eq!(
                result, state,
                "undocumented ({:?}, {:?}) must hold the source state; got {:?}",
                state, event, result,
            );
        }
    }

    /// Property 3: every path of documented events from Invoked either
    /// reaches a terminal Exit or stays inside the documented non-terminal
    /// state set — never escapes into an undocumented state.
    #[test]
    fn every_path_terminates_in_a_known_exit_code(
        events in prop::collection::vec(arb_documented_event(), 1..32),
    ) {
        let mut state = SignState::Invoked;
        for event in events {
            state = sign_transition(state, event);
            if let SignState::Exit(code) = state {
                prop_assert!(
                    is_terminal_allowed(code),
                    "terminal Exit code must be in the allowed set: got {:?}",
                    code,
                );
                return Ok(());
            }
        }
        // Did not terminate within 32 steps — fine; the property asserts
        // closure over the known state set, not forced termination.
        prop_assert!(
            sign_all_non_terminal_states().contains(&state),
            "non-terminal end state must be in the documented non-terminal set: got {:?}",
            state,
        );
    }

    /// Property 4: from Invoked, with arbitrary events (including
    /// undocumented ones that hold), any terminal Exit code is in the
    /// allowed set. Functionally proves the lifecycle never invents an
    /// exit code outside the diagram's matrix.
    #[test]
    fn exit_codes_are_in_the_allowed_set(
        events in prop::collection::vec(arb_event(), 1..64),
    ) {
        let mut state = SignState::Invoked;
        for event in events {
            state = sign_transition(state, event);
            if let SignState::Exit(code) = state {
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
// End-to-end smoke 5: `--offline` returns exit 3 + writes no bundle.
//
// Drives `commands::sign::run` with a stub manifest file (any file that
// passes `is_file()` works — the offline gate fires BEFORE the manifest is
// ever read). Asserts:
//   - return value == 3 (OfflineViolation),
//   - `<workspace>/bundles/` either doesn't exist OR is empty (no side
//     effect from the failed sign).
// No network, no OIDC required.
// ============================================================================

#[test]
fn offline_flag_returns_exit_3_and_writes_no_bundle() {
    let root = fresh_root("offline_smoke");
    // Stub manifest file: any non-empty file that passes is_file() is
    // sufficient — the offline gate at sign.rs:97 fires before any
    // manifest read.
    let manifest_path = root.join("stub.parquet");
    fs::write(&manifest_path, b"not really a parquet file").expect("write stub manifest");
    let workspace = root.join("workspace");

    let code = sign_run(SignArgs {
        manifest: manifest_path,
        workspace: Some(workspace.clone()),
        source_date_epoch: Some(1_748_109_600),
        oidc_token_file: None,
        offline: true,
        takedown_contact: None,
        dataset_homepage: None,
        publication_intent: None,
    });

    assert_eq!(code, 3, "expected exit 3 (OfflineViolation), got {code}");
    let bundle_dir = workspace.join("bundles");
    assert!(
        !bundle_dir.exists()
            || fs::read_dir(&bundle_dir)
                .map(|mut it| it.next().is_none())
                .unwrap_or(true),
        "no bundle file should be created when --offline returns exit 3"
    );
}

// ============================================================================
// End-to-end smoke 6: missing manifest file returns exit 2 before offline
// check + before any OIDC resolution.
//
// Locks the lifecycle ordering: args validation precedes the offline
// gate which precedes OIDC resolution. A user typo on the manifest path
// shouldn't surface as an identity error.
// ============================================================================

#[test]
fn missing_manifest_returns_exit_2_before_offline_or_oidc_resolution() {
    let root = fresh_root("missing_manifest");
    let nonexistent = root.join("nope.parquet");

    let code = sign_run(SignArgs {
        manifest: nonexistent,
        workspace: Some(root.join("workspace")),
        source_date_epoch: Some(1_748_109_600),
        oidc_token_file: None,
        // Both flags set: offline would normally exit 3, OIDC missing
        // would normally exit 4. Neither should fire because args
        // validation owns the first transition.
        offline: true,
        takedown_contact: None,
        dataset_homepage: None,
        publication_intent: None,
    });

    assert_eq!(
        code, 2,
        "expected exit 2 (ArgsError, missing manifest); got {code}"
    );
}
