//! W3C TDMRep parser (Text and Data Mining Reservation Protocol).
//!
//! Sprint 1 commit E10. Spec: W3C Community Final Report, 10 May 2024
//! (BUILD-PLAN §2.4). Five surfaces total; Sprint 1 implements three:
//!
//!   1. **`/.well-known/tdmrep.json`** — JSON array of `{location, tdm-reservation, tdm-policy?}`.
//!   2. **HTTP response header** — `tdm-reservation: 0|1` and `tdm-policy: <URL>` per resource.
//!   3. **HTML `<meta>` tag** — `<meta name="tdm-reservation" content="0|1">` (and `tdm-policy`).
//!
//! Deferred to Sprint 2+: EPUB 3 package metadata, PDF XMP. Both require
//! container-format readers (EPUB ZIP, PDF XMP) that distract from the
//! Sprint 1 signal-coverage goal.
//!
//! Processing rule (spec §3): TDM Agents MUST check well-known first; HTTP
//! header overrides per-resource; meta tag overrides per-page. Values other
//! than `"0"` and `"1"` are protocol errors → treated as unset (Unknown).
//!
//! The aggregator in `decision.rs` reconciles signals across surfaces — this
//! file's `evaluate` returns the strongest single-surface verdict.

use attestrum_core::{AttestrumError, Result};
use serde::{Deserialize, Serialize};

use crate::{SignalContext, SignalParser, SignalVerdict};

/// A single rule from `/.well-known/tdmrep.json`.
///
/// Per the W3C spec the `location` is robots.txt-like (supports `*` glob and
/// `$` anchor); Sprint 1 implements prefix-only matching like our robots.rs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WellKnownRule {
    pub location: String,
    #[serde(rename = "tdm-reservation")]
    pub tdm_reservation: u8,
    #[serde(rename = "tdm-policy")]
    #[serde(default)]
    pub tdm_policy: Option<String>,
}

/// Surface-specific parsed forms. Callers combine across surfaces using
/// `TdmRep::evaluate` (header > meta > well-known per spec §3 precedence).
#[derive(Debug, Clone, Default)]
pub struct TdmRep {
    /// Rules pulled from `/.well-known/tdmrep.json`.
    pub well_known: Vec<WellKnownRule>,
    /// Per-resource HTTP header value (`Some(0)`, `Some(1)`, or `None`).
    pub header_reservation: Option<u8>,
    /// Per-resource HTML `<meta>` value.
    pub meta_reservation: Option<u8>,
}

impl TdmRep {
    /// Parse a single well-known JSON payload. Empty payload → empty rule set
    /// (not an error). Non-UTF-8 or malformed JSON → `AttestrumError::Signal`.
    pub fn parse_well_known(bytes: &[u8]) -> Result<Vec<WellKnownRule>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        let raw: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|e| AttestrumError::Signal(format!("tdmrep.json invalid JSON: {e}")))?;
        let arr = raw
            .as_array()
            .ok_or_else(|| AttestrumError::Signal("tdmrep.json root must be an array".into()))?;
        let mut out = Vec::with_capacity(arr.len());
        for (i, v) in arr.iter().enumerate() {
            let rule: WellKnownRule = serde_json::from_value(v.clone()).map_err(|e| {
                AttestrumError::Signal(format!("tdmrep.json[{i}] schema error: {e}"))
            })?;
            // Per spec: values other than 0 and 1 are protocol errors.
            if rule.tdm_reservation > 1 {
                return Err(AttestrumError::Signal(format!(
                    "tdmrep.json[{i}].tdm-reservation must be 0 or 1, got {}",
                    rule.tdm_reservation
                )));
            }
            out.push(rule);
        }
        Ok(out)
    }

    /// Parse the value of a `tdm-reservation:` HTTP header. `"0"` or `"1"`
    /// returns `Some(value)`. Anything else returns `None` (per spec §3:
    /// invalid values treated as unset).
    pub fn parse_header_reservation(value: &str) -> Option<u8> {
        match value.trim() {
            "0" => Some(0),
            "1" => Some(1),
            _ => None,
        }
    }

    /// Parse an HTML body for `<meta name="tdm-reservation" content="...">`.
    /// Minimal in-tree extractor — no `quick-xml` / `scraper` dep. Returns
    /// `None` if not found or value is not `"0"`/`"1"`.
    pub fn parse_meta_reservation(html: &str) -> Option<u8> {
        let lower = html.to_ascii_lowercase();
        // Find `<meta ... name="tdm-reservation" ... content="...">` in any order.
        for start in find_all(&lower, "<meta") {
            let rest = &lower[start..];
            let Some(end) = rest.find('>') else {
                continue;
            };
            let tag = &rest[..end];
            if extract_attr(tag, "name") != Some("tdm-reservation".to_string()) {
                continue;
            }
            if let Some(content) = extract_attr(tag, "content") {
                return Self::parse_header_reservation(&content);
            }
        }
        None
    }

    /// Evaluate the strongest single-surface verdict for `path`. Precedence
    /// per W3C spec §3: HTTP header > HTML meta > well-known. Within
    /// well-known, longest-prefix-matching rule wins.
    pub fn evaluate(&self, path: &str) -> SignalVerdict {
        if let Some(v) = self.header_reservation {
            return reservation_to_verdict(v);
        }
        if let Some(v) = self.meta_reservation {
            return reservation_to_verdict(v);
        }
        let mut best: Option<&WellKnownRule> = None;
        for rule in &self.well_known {
            if !rule.location.is_empty() && path.starts_with(&rule.location) {
                let replace = match best {
                    None => true,
                    Some(b) => rule.location.len() > b.location.len(),
                };
                if replace {
                    best = Some(rule);
                }
            }
        }
        match best {
            Some(r) => reservation_to_verdict(r.tdm_reservation),
            None => SignalVerdict::Unknown,
        }
    }
}

fn reservation_to_verdict(v: u8) -> SignalVerdict {
    match v {
        0 => SignalVerdict::Allowed,
        1 => SignalVerdict::Disallowed,
        _ => SignalVerdict::Unknown,
    }
}

fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(idx) = haystack[from..].find(needle) {
        let abs = from + idx;
        out.push(abs);
        from = abs + needle.len();
    }
    out
}

/// Extract `key="value"` (or `key='value'`) from `attrs_str`. Returns the value
/// lower-cased for case-insensitive comparison.
fn extract_attr(attrs_str: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=");
    let idx = attrs_str.find(&pattern)?;
    let after = &attrs_str[idx + pattern.len()..];
    let (q, rest) = match after.as_bytes().first() {
        Some(b'"') => ('"', &after[1..]),
        Some(b'\'') => ('\'', &after[1..]),
        _ => return None,
    };
    let close = rest.find(q)?;
    Some(rest[..close].to_string())
}

/// `SignalParser` adapter — wraps the well-known JSON form. Callers parse
/// header + meta surfaces directly via `TdmRep::parse_header_reservation` /
/// `parse_meta_reservation`.
#[derive(Debug, Clone, Default)]
pub struct TdmRepParser;

impl TdmRepParser {
    pub fn new() -> Self {
        Self
    }
}

impl SignalParser for TdmRepParser {
    fn name(&self) -> &'static str {
        "tdmrep"
    }

    fn parse(&self, bytes: &[u8], context: &SignalContext) -> Result<SignalVerdict> {
        let rules = TdmRep::parse_well_known(bytes)?;
        let bundle = TdmRep {
            well_known: rules,
            header_reservation: None,
            meta_reservation: None,
        };
        Ok(bundle.evaluate(&context.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(path: &str) -> SignalContext {
        SignalContext::new("anyone", path)
    }

    #[test]
    fn parse_well_known_basic() {
        let json = br#"[{"location":"/","tdm-reservation":1}]"#;
        let rules = TdmRep::parse_well_known(json).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].location, "/");
        assert_eq!(rules[0].tdm_reservation, 1);
        assert!(rules[0].tdm_policy.is_none());
    }

    #[test]
    fn parse_well_known_with_policy() {
        let json =
            br#"[{"location":"/private","tdm-reservation":1,"tdm-policy":"https://example.com/p"}]"#;
        let rules = TdmRep::parse_well_known(json).unwrap();
        assert_eq!(
            rules[0].tdm_policy.as_deref(),
            Some("https://example.com/p")
        );
    }

    #[test]
    fn parse_well_known_empty_input_is_empty() {
        let rules = TdmRep::parse_well_known(b"").unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn parse_well_known_rejects_non_array_root() {
        assert!(TdmRep::parse_well_known(br#"{"location":"/"}"#).is_err());
    }

    #[test]
    fn parse_well_known_rejects_bad_reservation_value() {
        let json = br#"[{"location":"/","tdm-reservation":7}]"#;
        assert!(TdmRep::parse_well_known(json).is_err());
    }

    #[test]
    fn parse_header_reservation_accepts_zero_and_one() {
        assert_eq!(TdmRep::parse_header_reservation("0"), Some(0));
        assert_eq!(TdmRep::parse_header_reservation("1"), Some(1));
        assert_eq!(TdmRep::parse_header_reservation(" 1 "), Some(1));
    }

    #[test]
    fn parse_header_reservation_rejects_bogus() {
        assert_eq!(TdmRep::parse_header_reservation("2"), None);
        assert_eq!(TdmRep::parse_header_reservation("true"), None);
        assert_eq!(TdmRep::parse_header_reservation(""), None);
    }

    #[test]
    fn parse_meta_reservation_finds_value() {
        let html = r#"<html><head><meta name="tdm-reservation" content="1"></head></html>"#;
        assert_eq!(TdmRep::parse_meta_reservation(html), Some(1));

        let html2 = r#"<META NAME='tdm-reservation' CONTENT='0' />"#;
        assert_eq!(TdmRep::parse_meta_reservation(html2), Some(0));
    }

    #[test]
    fn parse_meta_reservation_missing_returns_none() {
        let html = r#"<html><head><meta charset="utf-8"></head></html>"#;
        assert_eq!(TdmRep::parse_meta_reservation(html), None);
    }

    #[test]
    fn evaluate_well_known_disallowed() {
        let bundle = TdmRep {
            well_known: vec![WellKnownRule {
                location: "/private".into(),
                tdm_reservation: 1,
                tdm_policy: None,
            }],
            ..Default::default()
        };
        assert_eq!(bundle.evaluate("/private/doc"), SignalVerdict::Disallowed);
        assert_eq!(bundle.evaluate("/public"), SignalVerdict::Unknown);
    }

    #[test]
    fn evaluate_well_known_longest_match_wins() {
        let bundle = TdmRep {
            well_known: vec![
                WellKnownRule {
                    location: "/".into(),
                    tdm_reservation: 0,
                    tdm_policy: None,
                },
                WellKnownRule {
                    location: "/private".into(),
                    tdm_reservation: 1,
                    tdm_policy: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(bundle.evaluate("/private/doc"), SignalVerdict::Disallowed);
        assert_eq!(bundle.evaluate("/public"), SignalVerdict::Allowed);
    }

    #[test]
    fn evaluate_http_header_overrides_well_known() {
        let bundle = TdmRep {
            well_known: vec![WellKnownRule {
                location: "/".into(),
                tdm_reservation: 0,
                tdm_policy: None,
            }],
            header_reservation: Some(1),
            ..Default::default()
        };
        assert_eq!(bundle.evaluate("/public"), SignalVerdict::Disallowed);
    }

    #[test]
    fn evaluate_html_meta_overrides_well_known_below_header() {
        let bundle = TdmRep {
            well_known: vec![WellKnownRule {
                location: "/".into(),
                tdm_reservation: 0,
                tdm_policy: None,
            }],
            meta_reservation: Some(1),
            ..Default::default()
        };
        assert_eq!(bundle.evaluate("/public"), SignalVerdict::Disallowed);
    }

    #[test]
    fn parser_trait_round_trip() {
        let p = TdmRepParser::new();
        let json = br#"[{"location":"/private","tdm-reservation":1}]"#;
        let v = p.parse(json, &ctx("/private/x")).unwrap();
        assert_eq!(v, SignalVerdict::Disallowed);
        assert_eq!(p.name(), "tdmrep");
    }
}
