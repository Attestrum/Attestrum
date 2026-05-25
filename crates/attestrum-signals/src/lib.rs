//! `attestrum-signals` — opt-out signal parsers (`robots.txt`, `ai.txt`, TDMRep, and more).
//!
//! Sprint 1 commit E8 ships the [`SignalParser`] trait, the [`SignalVerdict`]
//! enum, [`SignalContext`], and the first parser implementation: [`robots::RobotsParser`].
//! `ai.txt` lands in E9, TDMRep in E10. The cross-signal aggregator and the
//! `Ruleset` enum (`strict | audit-only | permissive` per BUILD-PLAN §0.5.3) land in
//! the same commit as TDMRep so all three parsers feed it at once.
//!
//! Parsers are deterministic and pure: bytes in, verdict out, no network. Fetch
//! orchestration is `attestrum-pipeline`'s job (Sprint 3).

use attestrum_core::Result;
use serde::{Deserialize, Serialize};

pub mod ai_txt;
pub mod decision;
pub mod robots;
pub mod tdmrep;

/// Per-signal verdict before any ruleset is applied. The aggregator in
/// `decision.rs` (lands E10) collapses multiple `SignalVerdict`s into a
/// terminal `SignalDecision` per BUILD-PLAN §0.5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalVerdict {
    /// The signal explicitly disallows AI training on this resource.
    Disallowed,
    /// The signal explicitly allows AI training on this resource.
    Allowed,
    /// The signal source was reachable + parseable but expressed no preference
    /// (e.g., robots.txt didn't list this user-agent), OR the source was
    /// unreachable (HTTP error) — per RFC 9309 Attestrum treats fetch error as
    /// `Unknown`, NOT as consent. Per-parser sub-rules (404, empty body, etc.)
    /// live in their `*-state.md` diagrams.
    Unknown,
}

/// Context the parser needs to evaluate a signal against a specific document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalContext {
    /// The bot identity we're asking on behalf of (e.g., `"GPTBot"`).
    /// Robots-style parsers match this against `User-Agent:` group headers.
    pub requested_user_agent: String,
    /// The path on the host being requested (e.g., `"/blog/post-1"`).
    /// For absolute URLs, callers strip scheme + host before populating this.
    pub path: String,
}

impl SignalContext {
    pub fn new(requested_user_agent: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            requested_user_agent: requested_user_agent.into(),
            path: path.into(),
        }
    }
}

/// A signal-source parser. Each implementation handles ONE wire format
/// (robots.txt, ai.txt, TDMRep JSON, IPTC PLUS XMP, etc.).
pub trait SignalParser {
    /// The static name of this signal (for failure messages, telemetry, and
    /// the `signalCoverage` field of the in-toto attestation predicate).
    fn name(&self) -> &'static str;

    /// Parse `bytes` against `context` and return a verdict.
    /// Returns `Err(AttestrumError::Signal(..))` for malformed input.
    fn parse(&self, bytes: &[u8], context: &SignalContext) -> Result<SignalVerdict>;
}

/// Curated AI-bot user-agent list, embedded at build time from
/// `src/data/ai_user_agents.txt`. Comments (`#`) and blank lines are stripped.
pub fn ai_user_agents() -> Vec<&'static str> {
    const RAW: &str = include_str!("data/ai_user_agents.txt");
    RAW.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_user_agents_includes_canonical_bots() {
        let list = ai_user_agents();
        for expected in [
            "GPTBot",
            "Google-Extended",
            "ClaudeBot",
            "CCBot",
            "PerplexityBot",
            "Applebot-Extended",
            "Bytespider",
            "Amazonbot",
            "cohere-ai",
        ] {
            assert!(list.contains(&expected), "missing: {expected}");
        }
    }

    #[test]
    fn ai_user_agents_excludes_comments_and_blanks() {
        let list = ai_user_agents();
        assert!(!list.iter().any(|s| s.starts_with('#')));
        assert!(!list.iter().any(|s| s.is_empty()));
    }

    #[test]
    fn signal_context_constructor() {
        let ctx = SignalContext::new("GPTBot", "/foo");
        assert_eq!(ctx.requested_user_agent, "GPTBot");
        assert_eq!(ctx.path, "/foo");
    }

    #[test]
    fn signal_verdict_round_trips_via_serde_json() {
        let v = SignalVerdict::Disallowed;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"Disallowed\"");
        let back: SignalVerdict = serde_json::from_str(&s).unwrap();
        assert_eq!(back, SignalVerdict::Disallowed);
    }
}
