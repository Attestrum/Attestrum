//! Regenerate the bounded fuzzy-match corpus artifact.
//!
//! ```text
//! cargo run -p fuzzy-web-gen -- tests/fixtures/fuzzy-web/fuzzy-index.json
//! ```
//!
//! Reads the curated showcase passages + `display.json` from
//! `tests/fixtures/showcase-passages/` and writes `fuzzy-index.json` to the
//! given path. The reproducibility test asserts the committed golden equals a
//! fresh run, so regenerate (and commit) whenever the passages or display
//! metadata change.

use std::path::PathBuf;
use std::process::ExitCode;

fn fixtures_dir() -> PathBuf {
    // <repo>/tools/fuzzy-web-gen → <repo>/tests/fixtures/showcase-passages
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/showcase-passages")
}

fn main() -> ExitCode {
    let Some(out) = std::env::args().nth(1) else {
        eprintln!("usage: fuzzy-web-gen <out-path/fuzzy-index.json>");
        return ExitCode::FAILURE;
    };

    let entries = match fuzzy_web_gen::load_entries(&fixtures_dir()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error reading showcase fixtures: {e}");
            return ExitCode::FAILURE;
        }
    };

    let index = fuzzy_web_gen::build_fuzzy_index(&entries, "wikitext-103-sealed");
    let json = fuzzy_web_gen::to_json(&index);

    if let Err(e) = std::fs::write(&out, &json) {
        eprintln!("error writing {out}: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "wrote {} leaves ({} bytes) to {out}",
        index.leaves.len(),
        json.len()
    );
    ExitCode::SUCCESS
}
