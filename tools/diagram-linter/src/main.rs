//! `diagram-linter` — enforces the diagram-first contract from PATH-A-BRIEF §0.3.
//!
//! Thin CLI shim — all check logic lives in [`diagram_linter`] so integration
//! tests under `tests/` can exercise it directly.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match parse(&args) {
        Ok(Command::Check { strict, json, root }) => run_check(strict, json, root),
        Ok(Command::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            print_help();
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Check {
        strict: bool,
        json: bool,
        root: PathBuf,
    },
    Help,
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter().skip(1);
    let Some(subcmd) = iter.next() else {
        return Ok(Command::Help);
    };
    match subcmd.as_str() {
        "check" => {
            let mut strict = false;
            let mut json = false;
            // Walks only the diagram directory by default. The handoff/audit/migration
            // markdown files under docs/ are prose, not diagrams, and don't carry the
            // required frontmatter or mermaid blocks. CI passes `--root docs/diagrams`
            // explicitly (see .github/workflows/ci.yml); this default matches.
            let mut root: PathBuf = PathBuf::from("docs/diagrams");
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--strict" => strict = true,
                    "--json" => json = true,
                    "--root" => {
                        let value = iter
                            .next()
                            .ok_or_else(|| "--root requires a value".to_string())?;
                        root = PathBuf::from(value);
                    }
                    "--help" | "-h" => return Ok(Command::Help),
                    other => return Err(format!("unknown flag: {other}")),
                }
            }
            Ok(Command::Check { strict, json, root })
        }
        "--help" | "-h" | "help" => Ok(Command::Help),
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn run_check(strict: bool, json: bool, root: PathBuf) -> ExitCode {
    match diagram_linter::run_check(&root, None, strict) {
        Ok(report) => {
            if json {
                report.print_json();
            } else {
                report.print_human();
            }
            ExitCode::from(report.exit_code())
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    eprintln!("diagram-linter — enforce the diagram-first contract (PATH-A-BRIEF §0.3)");
    eprintln!();
    eprintln!("usage: diagram-linter <subcommand> [flags]");
    eprintln!();
    eprintln!("subcommands:");
    eprintln!("  check   Run all enabled checks against *.md files under --root");
    eprintln!();
    eprintln!("flags (for `check`):");
    eprintln!("  --strict        Treat warnings as errors (exit non-zero on any check finding)");
    eprintln!("  --json          Emit a machine-readable JSON report on stdout");
    eprintln!("  --root <path>   Directory to walk (default: docs/diagrams)");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        std::iter::once("diagram-linter")
            .chain(parts.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn empty_args_yields_help() {
        assert_eq!(parse(&argv(&[])), Ok(Command::Help));
    }

    #[test]
    fn bare_check_uses_docs_diagrams_root() {
        assert_eq!(
            parse(&argv(&["check"])),
            Ok(Command::Check {
                strict: false,
                json: false,
                root: PathBuf::from("docs/diagrams"),
            })
        );
    }

    #[test]
    fn check_accepts_strict_json_root() {
        assert_eq!(
            parse(&argv(&[
                "check",
                "--strict",
                "--json",
                "--root",
                "fixtures/ok"
            ])),
            Ok(Command::Check {
                strict: true,
                json: true,
                root: PathBuf::from("fixtures/ok"),
            })
        );
    }

    #[test]
    fn root_without_value_errors() {
        assert!(parse(&argv(&["check", "--root"])).is_err());
    }

    #[test]
    fn unknown_flag_errors() {
        assert!(parse(&argv(&["check", "--nope"])).is_err());
    }
}
