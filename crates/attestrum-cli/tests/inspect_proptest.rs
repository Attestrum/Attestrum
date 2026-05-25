//! Property tests for the `attestrum inspect` lifecycle state machine.
//!
//! Sprint 3 commit E6. Closes the SECOND `stateDiagram-v2` proptest
//! obligation per CLAUDE.md §7.1 / PATH-A-BRIEF §7.1: every
//! `stateDiagram-v2` diagram must have a proptest enumerating its
//! transitions. First was Sprint 2 E2 for `signal-decision.md`
//! (`crates/attestrum-signals/tests/decision_proptest.rs`); this is for
//! `docs/diagrams/sprint-3/attestrum-inspect-lifecycle.md`.
//!
//! The proptests drive [`attestrum_cli::lifecycle`] (pure code, no I/O);
//! two exhaustive small-case tests at the end drive the actual
//! `attestrum inspect` binary against carefully-shaped manifest files to
//! lock the `Exit(Ok)` and `Exit(SchemaError)` paths end-to-end.

use std::fs::{self, File};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arrow::array::{Int32Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use attestrum_cas::CasStore;
use attestrum_cli::lifecycle::{
    all_events, all_non_terminal_states, documented_transitions, transition, ExitCode,
    InspectEvent, InspectState,
};
use attestrum_core::BuildContext;
use attestrum_pipeline::{build_corpus, CorpusEntry};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use parquet::format::KeyValue;
use proptest::prelude::*;

// ============================================================================
// CARGO_TARGET_TMPDIR scratch dirs (mirrors the pattern from
// crates/attestrum-cas/tests/store.rs).
// ============================================================================

static ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_root(test_name: &str) -> PathBuf {
    let n = ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    root.push(format!("attestrum-cli-e6-{test_name}-{n}"));
    if root.exists() {
        fs::remove_dir_all(&root).expect("cleanup prior test root");
    }
    fs::create_dir_all(&root).expect("create test root");
    root
}

fn attestrum_bin() -> &'static str {
    env!("CARGO_BIN_EXE_attestrum")
}

// ============================================================================
// proptest strategies
// ============================================================================

fn arb_event() -> impl Strategy<Value = InspectEvent> {
    let events = all_events();
    (0..events.len()).prop_map(move |i| events[i])
}

fn arb_state() -> impl Strategy<Value = InspectState> {
    let states = all_non_terminal_states();
    (0..states.len()).prop_map(move |i| states[i])
}

/// Only events that appear as the second element of a documented
/// transition tuple. Used by `proptest_every_path_terminates_in_a_known_exit_code`
/// so the random walks have a chance of reaching a terminal state.
fn arb_documented_event() -> impl Strategy<Value = InspectEvent> {
    let docs: Vec<InspectEvent> = documented_transitions()
        .iter()
        .map(|(_, e, _)| *e)
        .collect();
    let len = docs.len();
    (0..len).prop_map(move |i| docs[i])
}

fn is_documented(from: InspectState, event: InspectEvent) -> bool {
    documented_transitions()
        .iter()
        .any(|(s, e, _)| *s == from && *e == event)
}

fn is_terminal_allowed(code: ExitCode) -> bool {
    matches!(
        code,
        ExitCode::Ok | ExitCode::RuntimeError | ExitCode::ArgsError | ExitCode::SchemaError
    )
}

// ============================================================================
// Property 1: every documented transition is reachable
//
// Exhaustive (not actually a proptest — the documented set is finite
// and small). Locks the 10-edge spec from the diagram against the
// `transition` function so a divergence between code and diagram
// shows up here immediately.
// ============================================================================

#[test]
fn proptest_every_documented_transition_is_reachable() {
    for (from, event, expected_to) in documented_transitions() {
        let actual = transition(*from, *event);
        assert_eq!(
            actual, *expected_to,
            "documented transition ({from:?}, {event:?}) → expected {expected_to:?}, got {actual:?}"
        );
    }
}

// ============================================================================
// Property 2: no undocumented transition is taken (no silent forward progress)
//
// For any (state, event) pair NOT in the documented set, the lifecycle
// must HOLD — return the input state unchanged. The diagram's design
// choice is "hold" rather than "exit on unknown" so that bugs in event
// dispatch can't silently advance the machine past a missing edge.
// ============================================================================

proptest! {
    #[test]
    fn proptest_no_undocumented_transition_is_taken(
        state in arb_state(),
        event in arb_event(),
    ) {
        if !is_documented(state, event) {
            let result = transition(state, event);
            prop_assert_eq!(
                result, state,
                "undocumented ({:?}, {:?}) must hold the source state; got {:?}",
                state, event, result,
            );
        }
    }

    /// Property 3: every path of documented events from Invoked either
    /// reaches a terminal Exit or stays inside the documented
    /// non-terminal state set — never escapes into an undocumented
    /// state.
    ///
    /// Bounded at 32 random documented events. Documented events from
    /// states where the (state, event) pair isn't actually documented
    /// still hold (per property 2), so some random sequences may never
    /// terminate within the bound — that's fine, the property is about
    /// reachability of *known* states only.
    #[test]
    fn proptest_every_path_terminates_in_a_known_exit_code(
        events in prop::collection::vec(arb_documented_event(), 1..32),
    ) {
        let mut state = InspectState::Invoked;
        for event in events {
            state = transition(state, event);
            if let InspectState::Exit(code) = state {
                prop_assert!(
                    is_terminal_allowed(code),
                    "terminal Exit code must be in {{0, 1, 2, 8}}: got {:?}",
                    code,
                );
                return Ok(());
            }
        }
        // Did not terminate within 32 steps. That's still acceptable —
        // the property asserts CLOSURE over the known state set, not
        // forced termination. Confirm the resting state is one of the
        // documented non-terminals.
        prop_assert!(
            all_non_terminal_states().contains(&state),
            "non-terminal end state must be in the documented non-terminal set: got {:?}",
            state,
        );
    }

    /// Property 4: from Invoked, with arbitrary events from the FULL
    /// event set (including events not documented for the current
    /// state), any terminal state we reach carries an Exit code in
    /// the allowed set `{Ok, RuntimeError, ArgsError, SchemaError}` —
    /// never any other variant.
    ///
    /// Stronger than property 3 because it includes undocumented events
    /// (which hold). Functionally proves the lifecycle never invents
    /// an exit code outside the diagram's matrix.
    #[test]
    fn proptest_exit_codes_are_in_the_allowed_set(
        events in prop::collection::vec(arb_event(), 1..64),
    ) {
        let mut state = InspectState::Invoked;
        for event in events {
            state = transition(state, event);
            if let InspectState::Exit(code) = state {
                prop_assert!(
                    is_terminal_allowed(code),
                    "exit code outside allowed {{Ok, RuntimeError, ArgsError, SchemaError}}: got {:?}",
                    code,
                );
                return Ok(());
            }
        }
    }
}

// ============================================================================
// Exhaustive small-case 5: manifest with zero entries prints empty summary
// and exits 0. End-to-end against the compiled binary.
// ============================================================================

#[test]
fn manifest_with_zero_entries_prints_empty_summary_exits_0() {
    let root = fresh_root("empty_manifest");
    // Build an empty corpus to produce a valid, zero-row manifest. This
    // exercises the same write path as a real `attestrum build`, so the
    // resulting Parquet file matches the PROTECTED schema + writer
    // config from Sprint 3 E3.
    let ctx = BuildContext::new(root.clone(), 0);
    let cas = CasStore::new(root.join(".attestrum")).expect("CasStore::new");
    let out_dir = root.join(".attestrum").join("manifests");
    let entries: Vec<CorpusEntry> = Vec::new();
    let result =
        build_corpus(&ctx, &cas, &entries, &out_dir).expect("build empty corpus for fixture");

    let cmd = Command::new(attestrum_bin())
        .arg("inspect")
        .arg(&result.manifest_path)
        .output()
        .expect("spawn attestrum inspect");
    assert!(
        cmd.status.success(),
        "expected exit 0 on empty manifest; got {:?}\nstdout:\n{}\nstderr:\n{}",
        cmd.status.code(),
        String::from_utf8_lossy(&cmd.stdout),
        String::from_utf8_lossy(&cmd.stderr),
    );
    let stdout = String::from_utf8(cmd.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("leaf_count:  0"),
        "expected zero-leaf summary, got:\n{stdout}"
    );
    assert!(
        stdout.contains("total_bytes: 0"),
        "expected zero-bytes summary, got:\n{stdout}"
    );
    assert!(
        stdout.contains("per modality: (none)"),
        "expected empty modality histogram label, got:\n{stdout}"
    );
}

// ============================================================================
// Exhaustive small-case 6: manifest with unknown schema_version exits 8.
//
// Hand-crafts a Parquet file with:
//   - A minimal schema (one Int32 column, zero rows) so the file IS
//     valid Parquet but does NOT match `attestrum-manifest`'s 18-column
//     schema.
//   - File-level KeyValue metadata setting
//     `attestrum.manifest.schema_version = "999"` so
//     `attestrum_manifest::read_manifest_metadata` returns successfully
//     but the value mismatches `SCHEMA_VERSION = "2"`.
// This drives the Exit8 schema-mismatch path explicitly.
// ============================================================================

#[test]
fn manifest_with_unknown_schema_version_exits_8() {
    let root = fresh_root("schema_v999");
    let manifest_path = root.join("schema-mismatch.parquet");

    let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int32, true)]));
    let empty_col: Int32Array = vec![None::<i32>; 0].into();
    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(empty_col)])
        .expect("build empty record batch");

    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .set_key_value_metadata(Some(vec![
            KeyValue {
                key: "attestrum.manifest.schema_version".to_string(),
                value: Some("999".to_string()),
            },
            KeyValue {
                key: "attestrum.writer.profile".to_string(),
                value: Some("non-attestrum-test-fixture".to_string()),
            },
        ]))
        .build();

    let file = File::create(&manifest_path).expect("create fixture file");
    let mut writer = ArrowWriter::try_new(file, schema, Some(props)).expect("create arrow writer");
    writer.write(&batch).expect("write empty batch");
    writer.close().expect("close writer");

    let cmd = Command::new(attestrum_bin())
        .arg("inspect")
        .arg(&manifest_path)
        .output()
        .expect("spawn attestrum inspect");
    assert_eq!(
        cmd.status.code(),
        Some(8),
        "expected exit 8 (schema mismatch); got {:?}\nstdout:\n{}\nstderr:\n{}",
        cmd.status.code(),
        String::from_utf8_lossy(&cmd.stdout),
        String::from_utf8_lossy(&cmd.stderr),
    );
    let stderr = String::from_utf8_lossy(&cmd.stderr);
    assert!(
        stderr.contains("schema version mismatch"),
        "stderr should mention schema mismatch:\n{stderr}"
    );
}
