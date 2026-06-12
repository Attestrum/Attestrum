//! Determinism + golden coverage for the contamination report.
//!
//! Two guarantees, mirroring `attestrum-fingerprint`'s determinism suite:
//!   1. **Double-run byte-identity** — the same scan inputs serialize to the
//!      identical `report.json` bytes within a process (catches HashMap /
//!      rayon / env-order leakage).
//!   2. **Committed golden** — the report bytes match a checked-in golden, so
//!      the cross-target CI matrix surfaces any platform divergence.
//!
//! The fixture is built in-process with stable benchmark/corpus names (no
//! filesystem paths), so the golden is machine-independent. Regenerate with
//! `ATTESTRUM_REGEN_DECONTAMINATE_GOLDEN=1 cargo test -p attestrum-decontaminate
//! --test determinism`.

use attestrum_decontaminate::detect::{scan, BenchItem, Benchmark};
use attestrum_decontaminate::ingest::Doc;
use attestrum_decontaminate::report;
use std::collections::BTreeMap;
use std::path::PathBuf;

const GSM_Q1: &str = "natalia sold clips to 48 of her friends in april and then she sold half as many clips in may how many clips did natalia sell altogether in april and may";
const GSM_Q2: &str = "a robe takes two bolts of blue fiber and half that much white fiber how many bolts in total does it take to make the robe";
const ARC_A1: &str = "which property of a mineral can be determined simply by looking at a fresh unweathered sample under ordinary daylight conditions";

fn doc(id: &str, text: &str) -> Doc {
    Doc {
        id: id.into(),
        text: text.into(),
    }
}

/// Build the canonical fixture: two benchmarks, a corpus exercising every
/// signal pattern (verbatim → all three; embedded-in-filler → exact+contained,
/// not near; clean → no hit).
fn fixture_report_json() -> String {
    let gsm8k = Benchmark {
        name: "gsm8k".into(),
        items: vec![
            BenchItem::new("q1".into(), GSM_Q1),
            BenchItem::new("q2".into(), GSM_Q2),
        ],
    };
    let arc = Benchmark {
        name: "arc".into(),
        items: vec![BenchItem::new("a1".into(), ARC_A1)],
    };
    let benchmarks = vec![gsm8k, arc];

    let filler: String = (0..400).map(|i| format!("filler{i} ")).collect();
    let embedded = format!("{filler} {GSM_Q1} {filler}");
    let docs = vec![
        doc("verbatim", GSM_Q1),
        doc("embedded", &embedded),
        doc("clean", "the weather in lisbon was mild and the trams ran on time through the old town all afternoon today"),
        doc("arc-hit", ARC_A1),
    ];

    let bench_totals: BTreeMap<String, usize> =
        [("gsm8k".to_string(), 2usize), ("arc".to_string(), 1usize)]
            .into_iter()
            .collect();

    let (hits, stats) = scan(&docs, &benchmarks, 0.8);
    let report = report::build(
        vec!["corpus.jsonl".to_string()],
        stats,
        &bench_totals,
        &hits,
        0.8,
        None,
    );
    report.to_json().expect("report serialization")
}

#[test]
fn report_is_in_process_deterministic() {
    let first = fixture_report_json();
    let second = fixture_report_json();
    assert_eq!(
        first, second,
        "report.json bytes diverged across two in-process runs"
    );
}

#[test]
fn report_matches_committed_golden() {
    let derived = fixture_report_json();
    let path = goldens_dir().join("report.json");
    if std::env::var("ATTESTRUM_REGEN_DECONTAMINATE_GOLDEN").is_ok() {
        std::fs::write(&path, &derived).expect("regen write golden");
        eprintln!("regenerated {}", path.display());
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("read golden report.json");
    assert_eq!(
        derived,
        expected,
        "report.json differs from committed golden {}; if intended, regenerate with \
         ATTESTRUM_REGEN_DECONTAMINATE_GOLDEN=1",
        path.display()
    );
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}
