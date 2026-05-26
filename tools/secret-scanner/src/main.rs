//! `secret-scanner` CLI — sixth pre-commit gate per CLAUDE.md §7.
//!
//! Modes:
//!   - `secret-scanner check` (default): scan staged paths via git index.
//!     Returns non-zero if any pattern matches. This is the pre-commit hook
//!     mode.
//!   - `secret-scanner check --all`: scan the entire working tree. Used by
//!     CI as the post-push verification gate (so a disabled local hook
//!     doesn't open a hole).
//!   - `secret-scanner check --files <path>...`: scan specific files
//!     (working-tree content). For ad-hoc scans / debugging.

use std::path::PathBuf;
use std::process::ExitCode;

use secret_scanner::{
    scan_paths_staged, scan_paths_working_tree, staged_paths, standard_patterns, walk_working_tree,
    Finding,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match parse(&args) {
        Ok(Command::Check { mode }) => run_check(mode),
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
    Check { mode: Mode },
    Help,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Staged,
    All,
    Files(Vec<PathBuf>),
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter().skip(1);
    let Some(subcmd) = iter.next() else {
        return Ok(Command::Help);
    };
    match subcmd.as_str() {
        "check" => {
            let mut mode = Mode::Staged;
            let mut files: Vec<PathBuf> = Vec::new();
            let mut collecting_files = false;
            for arg in iter {
                match arg.as_str() {
                    "--all" => mode = Mode::All,
                    "--files" => collecting_files = true,
                    "--help" | "-h" => return Ok(Command::Help),
                    other if collecting_files => files.push(PathBuf::from(other)),
                    other => return Err(format!("unknown argument: {other}")),
                }
            }
            if !files.is_empty() {
                mode = Mode::Files(files);
            }
            Ok(Command::Check { mode })
        }
        "--help" | "-h" | "help" => Ok(Command::Help),
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn run_check(mode: Mode) -> ExitCode {
    let workspace_root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("secret-scanner: cannot read current dir: {e}");
            return ExitCode::from(2);
        }
    };
    let patterns = standard_patterns();

    let result: Result<Vec<Finding>, String> = match mode {
        Mode::Staged => {
            let paths = match staged_paths(&workspace_root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("secret-scanner: {e}");
                    return ExitCode::from(2);
                }
            };
            if paths.is_empty() {
                println!("secret-scanner: 0 staged files to check");
                return ExitCode::SUCCESS;
            }
            scan_paths_staged(&workspace_root, &paths, &patterns)
        }
        Mode::All => {
            let paths = match walk_working_tree(&workspace_root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("secret-scanner: {e}");
                    return ExitCode::from(2);
                }
            };
            Ok(scan_paths_working_tree(&workspace_root, &paths, &patterns))
        }
        Mode::Files(paths) => Ok(scan_paths_working_tree(&workspace_root, &paths, &patterns)),
    };

    let findings = match result {
        Ok(f) => f,
        Err(e) => {
            eprintln!("secret-scanner: {e}");
            return ExitCode::from(2);
        }
    };

    if findings.is_empty() {
        println!("secret-scanner: 0 findings");
        ExitCode::SUCCESS
    } else {
        eprintln!("secret-scanner: {} finding(s)", findings.len());
        for f in &findings {
            // Truncate matched string for display to avoid leaking the secret
            // into terminal scrollback / CI logs.
            let displayed = if f.matched.len() > 8 {
                format!("{}...({} chars)", &f.matched[..8], f.matched.len())
            } else {
                f.matched.clone()
            };
            eprintln!(
                "  {}:{}  [{}]  match={}",
                f.path.display(),
                f.line,
                f.pattern,
                displayed
            );
        }
        eprintln!();
        eprintln!("BLOCK: this commit would publish credential-shaped content.");
        eprintln!("See CLAUDE.md §0.5 (Publication Boundary) for the policy.");
        eprintln!("If a finding is a false positive, rewrite the line to use an");
        eprintln!("obvious placeholder (e.g., 'sk-...REPLACE_ME...').");
        ExitCode::from(1)
    }
}

fn print_help() {
    println!(
        "secret-scanner — credential pattern scanner (CLAUDE.md §7 sixth gate)\n\n\
USAGE:\n  \
  secret-scanner check                    # scan staged paths (default; pre-commit hook mode)\n  \
  secret-scanner check --all              # scan entire working tree (CI mode)\n  \
  secret-scanner check --files <path>...  # scan specific files\n\n\
Exit codes:\n  \
  0  no findings\n  \
  1  one or more findings (commit should be blocked)\n  \
  2  scanner / arg error"
    );
}
