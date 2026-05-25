//! Cross-signal aggregator + ruleset application.
//!
//! Sprint 1 commit E10. Implements the state machine drawn in
//! `docs/diagrams/overview/signal-decision.md`. Per-signal `SignalVerdict`s
//! collapse into a single per-document `SignalDecision` based on the chosen
//! `Ruleset`.
//!
//! Property test (deferred to Sprint 2): enumerate every
//! (signal-set × ruleset) pair and assert terminal state matches the diagram.

use serde::{Deserialize, Serialize};

use crate::SignalVerdict;

/// Per-corpus policy. Drives terminal-state resolution when a signal says
/// `Disallowed` or no signal expresses a preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ruleset {
    /// Reject anything that's not explicitly allowed by an opt-out signal.
    Strict,
    /// Include unknowns and disallows, but flag for human review.
    AuditOnly,
    /// Include everything, log overrides for the in-toto attestation.
    Permissive,
}

/// Terminal decision a single document receives after all signals + ruleset
/// have been applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalDecision {
    /// Include in the corpus, no special handling.
    Include,
    /// Include but flag (manifest's `signal_result` carries the source signal
    /// + ruleset that produced the flag).
    Flag { reason: String },
    /// Exclude from the corpus (manifest still records the row with
    /// `included=false` + `exclusion_reason`).
    Exclude { reason: String },
}

impl SignalDecision {
    pub fn is_included(&self) -> bool {
        matches!(self, SignalDecision::Include | SignalDecision::Flag { .. })
    }
}

/// Named verdict from a single signal source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalReport {
    pub source: &'static str, // e.g., "robots.txt"
    pub verdict: SignalVerdict,
}

/// Aggregate `reports` under `ruleset`. Implements the state machine from
/// `docs/diagrams/overview/signal-decision.md`:
///
///   any signal explicitly Disallowed:
///     Strict     → Exclude (with reason naming first disallowing signal)
///     AuditOnly  → Flag    (likewise)
///     Permissive → Flag    (logged but included)
///
///   else if any signal explicitly Allowed:
///     all rulesets → Include
///
///   else (all Unknown):
///     Strict     → Exclude
///     AuditOnly  → Flag
///     Permissive → Include
pub fn aggregate(reports: &[SignalReport], ruleset: Ruleset) -> SignalDecision {
    let first_disallow = reports
        .iter()
        .find(|r| r.verdict == SignalVerdict::Disallowed);
    if let Some(r) = first_disallow {
        let reason = format!("{} signal: Disallowed", r.source);
        return match ruleset {
            Ruleset::Strict => SignalDecision::Exclude { reason },
            Ruleset::AuditOnly | Ruleset::Permissive => SignalDecision::Flag { reason },
        };
    }
    if reports.iter().any(|r| r.verdict == SignalVerdict::Allowed) {
        return SignalDecision::Include;
    }
    let reason = "no signal expressed a preference (all Unknown)".to_string();
    match ruleset {
        Ruleset::Strict => SignalDecision::Exclude { reason },
        Ruleset::AuditOnly => SignalDecision::Flag { reason },
        Ruleset::Permissive => SignalDecision::Include,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(source: &'static str, verdict: SignalVerdict) -> SignalReport {
        SignalReport { source, verdict }
    }

    #[test]
    fn disallow_strict_excludes() {
        let reports = [r("robots.txt", SignalVerdict::Disallowed)];
        let d = aggregate(&reports, Ruleset::Strict);
        assert!(matches!(d, SignalDecision::Exclude { .. }));
    }

    #[test]
    fn disallow_audit_flags() {
        let reports = [r("robots.txt", SignalVerdict::Disallowed)];
        let d = aggregate(&reports, Ruleset::AuditOnly);
        match d {
            SignalDecision::Flag { reason } => assert!(reason.contains("robots.txt")),
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn disallow_permissive_flags_but_includes() {
        let reports = [r("ai.txt", SignalVerdict::Disallowed)];
        let d = aggregate(&reports, Ruleset::Permissive);
        assert!(matches!(d, SignalDecision::Flag { .. }));
        assert!(d.is_included());
    }

    #[test]
    fn allow_with_unknown_includes() {
        let reports = [
            r("robots.txt", SignalVerdict::Unknown),
            r("ai.txt", SignalVerdict::Allowed),
        ];
        let d = aggregate(&reports, Ruleset::Strict);
        assert_eq!(d, SignalDecision::Include);
    }

    #[test]
    fn all_unknown_strict_excludes() {
        let reports = [
            r("robots.txt", SignalVerdict::Unknown),
            r("ai.txt", SignalVerdict::Unknown),
        ];
        let d = aggregate(&reports, Ruleset::Strict);
        assert!(matches!(d, SignalDecision::Exclude { .. }));
    }

    #[test]
    fn all_unknown_audit_flags() {
        let reports = [r("robots.txt", SignalVerdict::Unknown)];
        let d = aggregate(&reports, Ruleset::AuditOnly);
        assert!(matches!(d, SignalDecision::Flag { .. }));
    }

    #[test]
    fn all_unknown_permissive_includes() {
        let reports = [r("robots.txt", SignalVerdict::Unknown)];
        let d = aggregate(&reports, Ruleset::Permissive);
        assert_eq!(d, SignalDecision::Include);
    }

    #[test]
    fn disallow_wins_over_allow() {
        // Per BUILD-PLAN §0.5.3: ANY disallow wins (conservative semantic).
        let reports = [
            r("robots.txt", SignalVerdict::Allowed),
            r("ai.txt", SignalVerdict::Disallowed),
        ];
        let d = aggregate(&reports, Ruleset::Strict);
        assert!(matches!(d, SignalDecision::Exclude { .. }));
    }

    #[test]
    fn is_included_helper() {
        assert!(SignalDecision::Include.is_included());
        assert!(SignalDecision::Flag { reason: "x".into() }.is_included());
        assert!(!SignalDecision::Exclude { reason: "x".into() }.is_included());
    }

    #[test]
    fn empty_reports_treated_as_all_unknown() {
        assert!(matches!(
            aggregate(&[], Ruleset::Strict),
            SignalDecision::Exclude { .. }
        ));
        assert_eq!(aggregate(&[], Ruleset::Permissive), SignalDecision::Include);
    }
}
