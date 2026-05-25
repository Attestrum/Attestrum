//! `ai.txt` parser (Spawning AI's convention).
//!
//! Sprint 1 commit E9. Format per BUILD-PLAN §2.2:
//!   - Plain text, served at the host's root (`/ai.txt`).
//!   - `User-Agent: <name>` groups (case-insensitive).
//!   - `Allow-AI-Training:` and `Disallow-AI-Training:` directives carrying
//!     either a path prefix (`/private`) OR a media-type glob (`image/*`,
//!     `text/*`, `*` for all).
//!   - `#` comments + blank lines.
//!
//! ai.txt is intended to be checked at the host of the linked MEDIA URL (not
//! just the HTML page), addressing the "third-party-hosted media" gap robots.txt
//! has. The caller is responsible for resolving the document's origin host;
//! this parser only sees bytes + context.

use attestrum_core::{AttestrumError, Result};

use crate::{SignalContext, SignalParser, SignalVerdict};

#[derive(Debug, Clone, Default)]
pub struct AiTxt {
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub agents: Vec<String>, // lowercased
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// Disallow AI training for the matching path / media-type pattern.
    Disallow(Pattern),
    /// Allow AI training for the matching pattern.
    Allow(Pattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Path prefix (starts with `/`), e.g., `/private`.
    Path(String),
    /// Media-type glob, e.g., `image/*`, `text/*`, `*` (= match anything).
    MediaType(String),
}

impl AiTxt {
    /// Parse `bytes` as UTF-8 ai.txt.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| AttestrumError::Signal(format!("ai.txt is not UTF-8: {e}")))?;
        let mut groups: Vec<Group> = Vec::new();
        let mut current: Option<Group> = None;
        let mut last_was_ua = false;
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
                    if !last_was_ua || current.is_none() {
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
                    last_was_ua = true;
                }
                "disallow-ai-training" => {
                    let g = current.get_or_insert_with(default_wildcard_group);
                    g.rules.push(Rule::Disallow(parse_pattern(&value)));
                    last_was_ua = false;
                }
                "allow-ai-training" => {
                    let g = current.get_or_insert_with(default_wildcard_group);
                    g.rules.push(Rule::Allow(parse_pattern(&value)));
                    last_was_ua = false;
                }
                // Ignore any unrecognized directives.
                _ => last_was_ua = false,
            }
        }
        if let Some(g) = current {
            groups.push(g);
        }
        Ok(Self { groups })
    }

    /// Evaluate whether `user_agent` may use `path` (with optional `mime`) for
    /// AI training. Same rule-resolution shape as `robots.txt`: specific UA
    /// beats `*`, then longest-pattern match wins, with `Allow` winning ties.
    pub fn evaluate(&self, user_agent: &str, path: &str, mime: Option<&str>) -> SignalVerdict {
        let ua = user_agent.to_ascii_lowercase();
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

        let mut best: Option<(usize, &Rule)> = None;
        for group in &groups {
            for rule in &group.rules {
                let pat = match rule {
                    Rule::Disallow(p) | Rule::Allow(p) => p,
                };
                if let Some(score) = match_score(pat, path, mime) {
                    let replace = match best {
                        None => true,
                        Some((best_score, Rule::Disallow(_))) => {
                            score > best_score
                                || (score == best_score && matches!(rule, Rule::Allow(_)))
                        }
                        Some((best_score, Rule::Allow(_))) => score > best_score,
                    };
                    if replace {
                        best = Some((score, rule));
                    }
                }
            }
        }
        match best {
            Some((_, Rule::Disallow(_))) => SignalVerdict::Disallowed,
            Some((_, Rule::Allow(_))) => SignalVerdict::Allowed,
            None => SignalVerdict::Allowed,
        }
    }
}

fn default_wildcard_group() -> Group {
    Group {
        agents: vec!["*".to_string()],
        rules: Vec::new(),
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn parse_pattern(value: &str) -> Pattern {
    if value == "*" || value.contains('/') && !value.starts_with('/') {
        // `*` or media-type-ish (`image/*`, `text/plain`).
        Pattern::MediaType(value.to_string())
    } else {
        // Treat anything else (path-shaped, including `/`) as a path.
        Pattern::Path(value.to_string())
    }
}

/// Return `Some(score)` if `pat` matches; `None` otherwise. Higher score = more specific.
fn match_score(pat: &Pattern, path: &str, mime: Option<&str>) -> Option<usize> {
    match pat {
        Pattern::Path(p) => {
            if p.is_empty() {
                None
            } else if path.starts_with(p) {
                Some(p.len() + 1000) // prefer path matches over media-type globs at same length
            } else {
                None
            }
        }
        Pattern::MediaType(pat) => {
            let m = mime?;
            if pat == "*" {
                Some(1)
            } else if let Some(prefix) = pat.strip_suffix("/*") {
                if m.to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
                {
                    Some(prefix.len() + 2)
                } else {
                    None
                }
            } else if pat.eq_ignore_ascii_case(m) {
                Some(pat.len() + 10)
            } else {
                None
            }
        }
    }
}

/// `SignalParser` adapter for ai.txt.
#[derive(Debug, Clone, Default)]
pub struct AiTxtParser;

impl AiTxtParser {
    pub fn new() -> Self {
        Self
    }
}

impl SignalParser for AiTxtParser {
    fn name(&self) -> &'static str {
        "ai.txt"
    }

    fn parse(&self, bytes: &[u8], context: &SignalContext) -> Result<SignalVerdict> {
        let parsed = AiTxt::parse(bytes)?;
        // ai.txt's media-type is conveyed out-of-band; SignalContext doesn't carry
        // mime today. Sprint 1 evaluates path-only. When fingerprinting lands in
        // Sprint 5 the context will gain `mime: Option<String>` and this call
        // updates in the same commit.
        Ok(parsed.evaluate(&context.requested_user_agent, &context.path, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ua: &str, path: &str) -> SignalContext {
        SignalContext::new(ua, path)
    }

    #[test]
    fn parses_minimal_disallow_all() {
        let r = AiTxt::parse(b"User-Agent: *\nDisallow-AI-Training: /\n").unwrap();
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].rules.len(), 1);
    }

    #[test]
    fn evaluate_path_prefix_disallowed() {
        let r = AiTxt::parse(b"User-Agent: GPTBot\nDisallow-AI-Training: /private\n").unwrap();
        assert_eq!(
            r.evaluate("GPTBot", "/private/doc", None),
            SignalVerdict::Disallowed
        );
        assert_eq!(
            r.evaluate("GPTBot", "/public/doc", None),
            SignalVerdict::Allowed
        );
    }

    #[test]
    fn evaluate_media_type_glob() {
        let r = AiTxt::parse(b"User-Agent: *\nDisallow-AI-Training: image/*\n").unwrap();
        assert_eq!(
            r.evaluate("anyone", "/photo.jpg", Some("image/jpeg")),
            SignalVerdict::Disallowed
        );
        assert_eq!(
            r.evaluate("anyone", "/doc.txt", Some("text/plain")),
            SignalVerdict::Allowed
        );
    }

    #[test]
    fn evaluate_unknown_agent_no_wildcard() {
        let r = AiTxt::parse(b"User-Agent: GPTBot\nDisallow-AI-Training: /\n").unwrap();
        assert_eq!(
            r.evaluate("ClaudeBot", "/anything", None),
            SignalVerdict::Unknown
        );
    }

    #[test]
    fn evaluate_specific_overrides_wildcard() {
        let r = AiTxt::parse(
            b"User-Agent: *\nDisallow-AI-Training: /\n\nUser-Agent: GPTBot\nAllow-AI-Training: /\n",
        )
        .unwrap();
        assert_eq!(
            r.evaluate("GPTBot", "/anything", None),
            SignalVerdict::Allowed
        );
        assert_eq!(
            r.evaluate("Mozilla", "/anything", None),
            SignalVerdict::Disallowed
        );
    }

    #[test]
    fn parse_pattern_classifies() {
        assert_eq!(parse_pattern("/foo"), Pattern::Path("/foo".into()));
        assert_eq!(
            parse_pattern("image/*"),
            Pattern::MediaType("image/*".into())
        );
        assert_eq!(parse_pattern("*"), Pattern::MediaType("*".into()));
        assert_eq!(
            parse_pattern("text/plain"),
            Pattern::MediaType("text/plain".into())
        );
    }

    #[test]
    fn ignores_unknown_directives_and_comments() {
        let r = AiTxt::parse(
            b"# top comment\nUser-Agent: *\nCrawl-delay: 1\nDisallow-AI-Training: /admin  # trailing\n",
        )
        .unwrap();
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].rules.len(), 1);
    }

    #[test]
    fn rejects_non_utf8() {
        assert!(AiTxt::parse(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn parser_trait_round_trip() {
        let p = AiTxtParser::new();
        let v = p
            .parse(
                b"User-Agent: GPTBot\nDisallow-AI-Training: /private\n",
                &ctx("GPTBot", "/private/foo"),
            )
            .unwrap();
        assert_eq!(v, SignalVerdict::Disallowed);
        assert_eq!(p.name(), "ai.txt");
    }

    #[test]
    fn empty_file_returns_unknown() {
        let r = AiTxt::parse(b"").unwrap();
        assert_eq!(r.evaluate("GPTBot", "/x", None), SignalVerdict::Unknown);
    }

    #[test]
    fn longest_match_wins_with_allow_tie_break() {
        let r =
            AiTxt::parse(b"User-Agent: *\nDisallow-AI-Training: /\nAllow-AI-Training: /public\n")
                .unwrap();
        assert_eq!(
            r.evaluate("GPTBot", "/public/x", None),
            SignalVerdict::Allowed
        );
        assert_eq!(
            r.evaluate("GPTBot", "/private/x", None),
            SignalVerdict::Disallowed
        );
    }
}
