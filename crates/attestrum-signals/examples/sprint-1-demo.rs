//! Sprint 1 acceptance demo per PATH-A-BRIEF Part 6. Exercises the three
//! parsers (`robots.txt`, `ai.txt`, TDMRep) against in-source fixtures, then
//! aggregates the verdicts under `Ruleset::Strict`.
//!
//! Run via: `cargo run --example sprint-1-demo --quiet`
//! Recording: `docs/demos/sprint-1.cast`

use attestrum_signals::ai_txt::AiTxtParser;
use attestrum_signals::decision::{aggregate, Ruleset, SignalReport};
use attestrum_signals::robots::RobotsParser;
use attestrum_signals::tdmrep::TdmRepParser;
use attestrum_signals::{SignalContext, SignalParser};

fn main() {
    println!("=== Attestrum Sprint 1 demo: 3 signal parsers ===");
    println!();

    let mut reports = Vec::new();

    // 1. robots.txt fixture.
    let robots_bytes = b"User-Agent: GPTBot\nDisallow: /private\n";
    let robots_ctx = SignalContext::new("GPTBot", "/private/secret.html");
    let robots_parser = RobotsParser::new();
    let robots_verdict = robots_parser.parse(robots_bytes, &robots_ctx).unwrap();
    println!("[1/3] robots.txt");
    println!("  Input:   User-Agent: GPTBot");
    println!("           Disallow: /private");
    println!("  Query:   GPTBot @ /private/secret.html");
    println!("  Verdict: {robots_verdict:?}");
    println!();
    reports.push(SignalReport {
        source: "robots.txt",
        verdict: robots_verdict,
    });

    // 2. ai.txt fixture.
    let aitxt_bytes = b"User-Agent: *\nDisallow-AI-Training: /\n";
    let aitxt_ctx = SignalContext::new("any-bot", "/anywhere");
    let aitxt_parser = AiTxtParser::new();
    let aitxt_verdict = aitxt_parser.parse(aitxt_bytes, &aitxt_ctx).unwrap();
    println!("[2/3] ai.txt");
    println!("  Input:   User-Agent: *");
    println!("           Disallow-AI-Training: /");
    println!("  Query:   any-bot @ /anywhere");
    println!("  Verdict: {aitxt_verdict:?}");
    println!();
    reports.push(SignalReport {
        source: "ai.txt",
        verdict: aitxt_verdict,
    });

    // 3. TDMRep well-known JSON fixture.
    let tdmrep_bytes = br#"[{"location":"/private","tdm-reservation":1}]"#;
    let tdmrep_ctx = SignalContext::new("anyone", "/private/doc.html");
    let tdmrep_parser = TdmRepParser::new();
    let tdmrep_verdict = tdmrep_parser.parse(tdmrep_bytes, &tdmrep_ctx).unwrap();
    println!("[3/3] TDMRep well-known JSON");
    println!("  Input:   [{{\"location\":\"/private\",\"tdm-reservation\":1}}]");
    println!("  Query:   /private/doc.html");
    println!("  Verdict: {tdmrep_verdict:?}");
    println!();
    reports.push(SignalReport {
        source: "tdmrep",
        verdict: tdmrep_verdict,
    });

    // Aggregate under each ruleset to show the state-machine outcome.
    println!("--- Cross-signal aggregation ---");
    for ruleset in [Ruleset::Strict, Ruleset::AuditOnly, Ruleset::Permissive] {
        let decision = aggregate(&reports, ruleset);
        println!("  {ruleset:?}: {decision:?}");
    }
    println!();
    println!("Done. (Sprint 1 acceptance: parser output for 3 fixtures + aggregator.)");
}
