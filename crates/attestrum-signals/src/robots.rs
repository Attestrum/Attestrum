//! `robots.txt` parser per RFC 9309, with AI-user-agent matching.
//!
//! Sprint 1 commit E8. In-tree implementation — no `robotstxt` crate dep.
//! ~150 LOC including matching logic. Covers the cases BUILD-PLAN §2.1 calls out:
//! AI-bot User-Agent groups, Disallow/Allow rules, the `User-Agent: *` wildcard
//! group, comments, and the edge cases documented in
//! `docs/diagrams/sprint-1/robots-txt-state.md`.
//!
//! Out of scope for Sprint 1: `Crawl-delay`, `Sitemap`, AIPref `Content-Usage`
//! header semantics (AIPref lands in Sprint 4 per BUILD-PLAN §9 Sprint 1 risk
//! note). Cloudflare Content-Signals comment-block parsing folds in here when
//! that signal lands in a later sprint.

use attestrum_core::{AttestrumError, Result};

use crate::{SignalContext, SignalParser, SignalVerdict};

/// A parsed robots.txt — collection of groups, each keyed by user-agent names.
#[derive(Debug, Clone, Default)]
pub struct RobotsTxt {
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone)]
pub struct Group {
    /// User-Agent names this group applies to (lowercased, comparable case-insensitively
    /// per RFC 9309 §2.2.1).
    pub agents: Vec<String>,
    /// Rules in the order they appeared. Order matters for ambiguous matches —
    /// we apply the "most-specific path" rule per RFC 9309 §2.2.2, breaking ties
    /// by source order.
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Disallow(String),
    Allow(String),
}

impl RobotsTxt {
    /// Parse `bytes` as UTF-8 robots.txt. Returns `Err` only if the bytes
    /// are not valid UTF-8 — every other oddity becomes an empty/unset group.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| AttestrumError::Signal(format!("robots.txt is not UTF-8: {e}")))?;
        let mut groups: Vec<Group> = Vec::new();
        let mut current: Option<Group> = None;
        let mut last_directive_was_ua = false;
        for raw_line in text.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "user-agent" => {
                    if !last_directive_was_ua || current.is_none() {
                        // Start a fresh group (a non-UA directive last seen, or first group).
                        if let Some(g) = current.take() {
                            groups.push(g);
                        }
                        current = Some(Group {
                            agents: Vec::new(),
                            rules: Vec::new(),
                        });
                    }
                    if let Some(g) = current.as_mut() {
                        g.agents.push(value.to_ascii_lowercase());
                    }
                    last_directive_was_ua = true;
                }
                "disallow" => {
                    let g = current.get_or_insert_with(|| Group {
                        agents: vec!["*".to_string()],
                        rules: Vec::new(),
                    });
                    g.rules.push(Rule::Disallow(value));
                    last_directive_was_ua = false;
                }
                "allow" => {
                    let g = current.get_or_insert_with(|| Group {
                        agents: vec!["*".to_string()],
                        rules: Vec::new(),
                    });
                    g.rules.push(Rule::Allow(value));
                    last_directive_was_ua = false;
                }
                // Ignore Crawl-delay, Sitemap, Host, and any unrecognized directives.
                _ => last_directive_was_ua = false,
            }
        }
        if let Some(g) = current {
            groups.push(g);
        }
        Ok(Self { groups })
    }

    /// True iff `user_agent` is explicitly listed in any group OR a wildcard
    /// `*` group exists. Case-insensitive.
    pub fn knows_agent(&self, user_agent: &str) -> bool {
        let ua = user_agent.to_ascii_lowercase();
        self.groups
            .iter()
            .any(|g| g.agents.iter().any(|a| a == &ua || a == "*"))
    }

    /// Evaluate whether `user_agent` may access `path`. Returns:
    ///   - `Disallowed` if a matching group has a `Disallow:` rule covering `path`
    ///     that is more specific than any matching `Allow:`.
    ///   - `Allowed`    if a matching group exists and rules permit `path`
    ///     (either no Disallow applies, or an Allow is more specific).
    ///   - `Unknown`    if NO group matches this user-agent (no specific entry
    ///     AND no `User-Agent: *`). Per RFC 9309 §2.2.1 absent means "may access";
    ///     Attestrum chooses `Unknown` here so the ruleset layer decides per
    ///     BUILD-PLAN §0.5.3.
    pub fn evaluate(&self, user_agent: &str, path: &str) -> SignalVerdict {
        let ua = user_agent.to_ascii_lowercase();
        // Per RFC 9309 §2.2.1: prefer specific UA group over `*` if both exist.
        let specific: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == &ua))
            .collect();
        let wildcard: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == "*"))
            .collect();
        let groups: Vec<&Group> = if !specific.is_empty() {
            specific
        } else if !wildcard.is_empty() {
            wildcard
        } else {
            return SignalVerdict::Unknown;
        };

        // Collect all matching rules across selected groups; pick the most-specific
        // (longest pattern) match, breaking ties by Allow > Disallow per RFC 9309 §2.2.2.
        let mut best: Option<(usize, &Rule)> = None;
        for group in &groups {
            for rule in &group.rules {
                let pattern = match rule {
                    Rule::Disallow(p) => p,
                    Rule::Allow(p) => p,
                };
                if path_matches(pattern, path) {
                    let len = pattern.len();
                    let replace = match best {
                        None => true,
                        Some((best_len, Rule::Disallow(_))) => {
                            len > best_len || (len == best_len && matches!(rule, Rule::Allow(_)))
                        }
                        Some((best_len, Rule::Allow(_))) => len > best_len,
                    };
                    if replace {
                        best = Some((len, rule));
                    }
                }
            }
        }
        match best {
            Some((_, Rule::Disallow(_))) => SignalVerdict::Disallowed,
            Some((_, Rule::Allow(_))) => SignalVerdict::Allowed,
            None => SignalVerdict::Allowed, // matching group exists but no rule fires → permitted
        }
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn path_matches(pattern: &str, path: &str) -> bool {
    // RFC 9309 §2.2.2: simple prefix match. Empty pattern matches nothing
    // (a bare `Disallow:` means "allow all", which we model by `pattern.is_empty()` → no match).
    if pattern.is_empty() {
        return false;
    }
    // We don't yet support the `$` end-anchor or `*` glob (RFC 9309 §2.2.3 extensions).
    // Sprint 1 scope: prefix-only. Extensions land when a real-world fixture needs them.
    path.starts_with(pattern)
}

/// `SignalParser` implementation for robots.txt.
#[derive(Debug, Clone, Default)]
pub struct RobotsParser;

impl RobotsParser {
    pub fn new() -> Self {
        Self
    }
}

impl SignalParser for RobotsParser {
    fn name(&self) -> &'static str {
        "robots.txt"
    }

    fn parse(&self, bytes: &[u8], context: &SignalContext) -> Result<SignalVerdict> {
        let parsed = RobotsTxt::parse(bytes)?;
        Ok(parsed.evaluate(&context.requested_user_agent, &context.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ua: &str, path: &str) -> SignalContext {
        SignalContext::new(ua, path)
    }

    #[test]
    fn parses_simple_disallow() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow: /private\n").unwrap();
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].agents, vec!["gptbot"]);
        assert_eq!(r.groups[0].rules, vec![Rule::Disallow("/private".into())]);
    }

    #[test]
    fn parses_multiple_groups() {
        let r = RobotsTxt::parse(
            b"User-Agent: GPTBot\nDisallow: /private\n\nUser-Agent: *\nAllow: /\n",
        )
        .unwrap();
        assert_eq!(r.groups.len(), 2);
    }

    #[test]
    fn parses_combined_user_agent_block() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nUser-Agent: ClaudeBot\nDisallow: /no-ai\n")
            .unwrap();
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].agents, vec!["gptbot", "claudebot"]);
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let r = RobotsTxt::parse(b"# top comment\n\nUser-Agent: *\nDisallow: /admin  # trailing\n")
            .unwrap();
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].rules, vec![Rule::Disallow("/admin".into())]);
    }

    #[test]
    fn rejects_non_utf8() {
        let bytes = &[0xff, 0xfe, 0x00];
        assert!(RobotsTxt::parse(bytes).is_err());
    }

    #[test]
    fn evaluate_specific_disallow() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow: /private\n").unwrap();
        assert_eq!(
            r.evaluate("GPTBot", "/private/document.html"),
            SignalVerdict::Disallowed
        );
        assert_eq!(r.evaluate("GPTBot", "/public/foo"), SignalVerdict::Allowed);
    }

    #[test]
    fn evaluate_unknown_agent_no_wildcard_returns_unknown() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow: /private\n").unwrap();
        assert_eq!(r.evaluate("ClaudeBot", "/anything"), SignalVerdict::Unknown);
    }

    #[test]
    fn evaluate_unknown_agent_with_wildcard_uses_wildcard() {
        let r =
            RobotsTxt::parse(b"User-Agent: *\nDisallow: /admin\n\nUser-Agent: GPTBot\nAllow: /\n")
                .unwrap();
        assert_eq!(
            r.evaluate("ClaudeBot", "/admin/secret"),
            SignalVerdict::Disallowed
        );
    }

    #[test]
    fn evaluate_specific_overrides_wildcard() {
        let r = RobotsTxt::parse(b"User-Agent: *\nDisallow: /\n\nUser-Agent: GPTBot\nAllow: /\n")
            .unwrap();
        assert_eq!(r.evaluate("GPTBot", "/anything"), SignalVerdict::Allowed);
        assert_eq!(
            r.evaluate("Mozilla", "/anything"),
            SignalVerdict::Disallowed
        );
    }

    #[test]
    fn evaluate_longest_match_wins() {
        let r = RobotsTxt::parse(b"User-Agent: *\nDisallow: /\nAllow: /public\n").unwrap();
        // /public matched by Allow:/public (len 7) and Disallow:/ (len 1) — Allow wins.
        assert_eq!(r.evaluate("GPTBot", "/public/x"), SignalVerdict::Allowed);
        // / matched only by Disallow:/.
        assert_eq!(
            r.evaluate("GPTBot", "/private/x"),
            SignalVerdict::Disallowed
        );
    }

    #[test]
    fn evaluate_is_case_insensitive_on_agent() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow: /x\n").unwrap();
        assert_eq!(r.evaluate("gptbot", "/x"), SignalVerdict::Disallowed);
        assert_eq!(r.evaluate("GPTBOT", "/x"), SignalVerdict::Disallowed);
    }

    #[test]
    fn parser_trait_round_trip() {
        let p = RobotsParser::new();
        let v = p
            .parse(
                b"User-Agent: GPTBot\nDisallow: /private\n",
                &ctx("GPTBot", "/private/foo"),
            )
            .unwrap();
        assert_eq!(v, SignalVerdict::Disallowed);
        assert_eq!(p.name(), "robots.txt");
    }

    #[test]
    fn empty_robots_returns_unknown() {
        let r = RobotsTxt::parse(b"").unwrap();
        assert_eq!(r.evaluate("GPTBot", "/anything"), SignalVerdict::Unknown);
    }

    #[test]
    fn bare_disallow_means_allow_all() {
        // RFC 9309: `Disallow:` with empty value is equivalent to allowing all.
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow:\n").unwrap();
        assert_eq!(r.evaluate("GPTBot", "/anywhere"), SignalVerdict::Allowed);
    }

    #[test]
    fn knows_agent_detects_specific_and_wildcard() {
        let r = RobotsTxt::parse(b"User-Agent: GPTBot\nDisallow: /\n").unwrap();
        assert!(r.knows_agent("GPTBot"));
        assert!(r.knows_agent("gptbot"));
        assert!(!r.knows_agent("Mozilla"));

        let r2 = RobotsTxt::parse(b"User-Agent: *\nDisallow: /\n").unwrap();
        assert!(r2.knows_agent("anyone"));
    }
}
