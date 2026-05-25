//! Property test for the cross-signal aggregator.
//!
//! Sprint 2 commit E2. Fulfills the obligation deferred from Sprint 1 E10
//! and codified in CLAUDE.md §7.1 / PATH-A-BRIEF §7.1: every `stateDiagram-v2`
//! diagram must have a proptest enumerating its transitions. The diagram in
//! question is `docs/diagrams/overview/signal-decision.md`.
//!
//! The aggregator state machine collapses N per-signal verdicts (Disallowed
//! / Allowed / Unknown) + 1 corpus-wide `Ruleset` (Strict / AuditOnly /
//! Permissive) into a single terminal `SignalDecision` (Exclude / Flag /
//! Include). The properties below exhaustively cover the (signal-set ×
//! ruleset) space:
//!
//!   1. ANY Disallowed signal wins (conservative semantic). Strict → Exclude;
//!      AuditOnly and Permissive → Flag with reason.
//!   2. No Disallowed + at least one Allowed → Include under every ruleset.
//!   3. All Unknown (including the empty list) → ruleset-driven terminal:
//!      Strict → Exclude; AuditOnly → Flag; Permissive → Include.
//!   4. When a Disallowed report is present, the reason string mentions the
//!      *first* such report's source signal (deterministic ordering — the
//!      iterator finds the leftmost Disallowed).
//!
//! A final exhaustive (non-proptest) sanity check covers the trivial 3×3
//! single-verdict × ruleset matrix end-to-end.

use attestrum_signals::decision::{aggregate, Ruleset, SignalDecision, SignalReport};
use attestrum_signals::SignalVerdict;
use proptest::prelude::*;

/// Pool of source names the property tests sample from. Mirrors the Sprint 1
/// parser surface (robots.txt + ai.txt + TDMRep across its three transport
/// modes) — enough variety to exercise the aggregator without leaking into
/// parsers themselves.
const SOURCES: &[&str] = &[
    "robots.txt",
    "ai.txt",
    "tdmrep-well-known",
    "tdmrep-header",
    "tdmrep-meta",
];

fn arb_verdict() -> impl Strategy<Value = SignalVerdict> {
    prop_oneof![
        Just(SignalVerdict::Disallowed),
        Just(SignalVerdict::Allowed),
        Just(SignalVerdict::Unknown),
    ]
}

fn arb_ruleset() -> impl Strategy<Value = Ruleset> {
    prop_oneof![
        Just(Ruleset::Strict),
        Just(Ruleset::AuditOnly),
        Just(Ruleset::Permissive),
    ]
}

fn arb_report() -> impl Strategy<Value = SignalReport> {
    (0..SOURCES.len(), arb_verdict()).prop_map(|(i, verdict)| SignalReport {
        source: SOURCES[i],
        verdict,
    })
}

fn arb_reports() -> impl Strategy<Value = Vec<SignalReport>> {
    prop::collection::vec(arb_report(), 0..16)
}

proptest! {
    /// Property 1: ANY Disallowed wins (conservative semantic).
    #[test]
    fn any_disallowed_wins(
        reports in arb_reports(),
        ruleset in arb_ruleset(),
    ) {
        let has_disallow = reports
            .iter()
            .any(|r| r.verdict == SignalVerdict::Disallowed);
        if !has_disallow {
            return Ok(());
        }
        let decision = aggregate(&reports, ruleset);
        match ruleset {
            Ruleset::Strict => prop_assert!(
                matches!(decision, SignalDecision::Exclude { .. }),
                "Strict + Disallowed must Exclude; got {decision:?}",
            ),
            Ruleset::AuditOnly | Ruleset::Permissive => prop_assert!(
                matches!(decision, SignalDecision::Flag { .. }),
                "{ruleset:?} + Disallowed must Flag; got {decision:?}",
            ),
        }
    }

    /// Property 2: No Disallowed + at least one Allowed → Include under every ruleset.
    #[test]
    fn allowed_includes_when_no_disallow(
        reports in arb_reports(),
        ruleset in arb_ruleset(),
    ) {
        let has_disallow = reports
            .iter()
            .any(|r| r.verdict == SignalVerdict::Disallowed);
        let has_allow = reports
            .iter()
            .any(|r| r.verdict == SignalVerdict::Allowed);
        if !has_disallow && has_allow {
            prop_assert_eq!(
                aggregate(&reports, ruleset),
                SignalDecision::Include,
                "no-Disallow + has-Allow must Include under {:?}",
                ruleset,
            );
        }
    }

    /// Property 3: All Unknown (including the empty list) → ruleset-driven.
    #[test]
    fn all_unknown_follows_ruleset(
        reports in arb_reports(),
        ruleset in arb_ruleset(),
    ) {
        let all_unknown = reports
            .iter()
            .all(|r| r.verdict == SignalVerdict::Unknown);
        if !all_unknown {
            return Ok(());
        }
        let decision = aggregate(&reports, ruleset);
        match ruleset {
            Ruleset::Strict => prop_assert!(
                matches!(decision, SignalDecision::Exclude { .. }),
                "Strict + all-Unknown (n={}) must Exclude; got {decision:?}",
                reports.len(),
            ),
            Ruleset::AuditOnly => prop_assert!(
                matches!(decision, SignalDecision::Flag { .. }),
                "AuditOnly + all-Unknown (n={}) must Flag; got {decision:?}",
                reports.len(),
            ),
            Ruleset::Permissive => prop_assert_eq!(
                decision,
                SignalDecision::Include,
                "Permissive + all-Unknown (n={}) must Include",
                reports.len(),
            ),
        }
    }

    /// Property 4: When Disallowed is present, the reason string names the
    /// *first* (leftmost) Disallowed report's source. Determinism check on
    /// the aggregator's ordering — `.iter().find(...)` is left-to-right.
    #[test]
    fn disallow_reason_names_first_source(
        reports in arb_reports(),
        ruleset in arb_ruleset(),
    ) {
        let first_disallow = reports
            .iter()
            .find(|r| r.verdict == SignalVerdict::Disallowed);
        let Some(first) = first_disallow else { return Ok(()); };
        let decision = aggregate(&reports, ruleset);
        let reason = match &decision {
            SignalDecision::Exclude { reason } => reason,
            SignalDecision::Flag { reason } => reason,
            SignalDecision::Include => {
                prop_assert!(false, "Disallowed present but decision is Include: {decision:?}");
                return Ok(());
            }
        };
        prop_assert!(
            reason.contains(first.source),
            "reason '{reason}' should mention first Disallow source '{}'",
            first.source,
        );
    }
}

/// Exhaustive sanity check on the trivial 3-verdicts × 3-rulesets matrix
/// (single-report case). Complements the proptests above by removing all
/// randomness from the most-common single-signal scenario.
#[test]
fn exhaustive_single_report_matrix() {
    use Ruleset::*;
    use SignalVerdict::*;
    let cases: &[(SignalVerdict, Ruleset, &str)] = &[
        (Disallowed, Strict, "Exclude"),
        (Disallowed, AuditOnly, "Flag"),
        (Disallowed, Permissive, "Flag"),
        (Allowed, Strict, "Include"),
        (Allowed, AuditOnly, "Include"),
        (Allowed, Permissive, "Include"),
        (Unknown, Strict, "Exclude"),
        (Unknown, AuditOnly, "Flag"),
        (Unknown, Permissive, "Include"),
    ];
    for (verdict, ruleset, expected) in cases {
        let reports = [SignalReport {
            source: "exhaustive-test",
            verdict: *verdict,
        }];
        let decision = aggregate(&reports, *ruleset);
        let got = match decision {
            SignalDecision::Exclude { .. } => "Exclude",
            SignalDecision::Flag { .. } => "Flag",
            SignalDecision::Include => "Include",
        };
        assert_eq!(got, *expected, "matrix cell ({verdict:?}, {ruleset:?})");
    }
}

/// Empty report list is treated as the all-Unknown case (no signal expressed
/// a preference). Diagram and code both encode this as the empty-input edge.
#[test]
fn empty_reports_match_all_unknown() {
    use Ruleset::*;
    assert!(matches!(
        aggregate(&[], Strict),
        SignalDecision::Exclude { .. }
    ));
    assert!(matches!(
        aggregate(&[], AuditOnly),
        SignalDecision::Flag { .. }
    ));
    assert_eq!(aggregate(&[], Permissive), SignalDecision::Include);
}
