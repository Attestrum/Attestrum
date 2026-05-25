//! `attestrum` — top-level CLI binary. Sprint 3 ships `build` (E5),
//! `inspect` (E6), `plan` + `merge` (E7); later sprints add `sign`,
//! `verify`, `prove`, `publish`. The actual subcommand implementations
//! live in the `attestrum_cli` library under [`attestrum_cli::commands`].
//!
//! Exit-code convention (mirrors BUILD-PLAN §8.4 + the per-subcommand
//! diagrams under `docs/diagrams/sprint-3/`):
//!
//! - `0` — success.
//! - `1` — runtime error (TOML parse, missing corpus file for `build`,
//!   pipeline failure, write failure, parquet I/O error for `inspect`).
//! - `2` — argument-style error (clap parse failure; for `inspect` also
//!   manifest-path-missing-or-not-a-file per
//!   `attestrum-inspect-lifecycle.md`).
//! - `3` — `--offline` violation (no-op in v1; reserved for Sprint 4
//!   network fetch layer).
//! - `7` — determinism failure slot (reserved; only surfaced by the
//!   cross-platform CI matrix, not by any subcommand in isolation).
//! - `8` — schema-version mismatch (`inspect` only; the manifest file
//!   is valid Parquet but its `attestrum.manifest.schema_version` KeyValue
//!   does not match `attestrum_manifest::SCHEMA_VERSION`).

use std::path::PathBuf;
use std::process::ExitCode;

use attestrum_cli::commands;
use clap::{Parser, Subcommand};

/// Attestrum — deterministic Rust CLI for AI training-data provenance
/// bundles.
#[derive(Parser, Debug)]
#[command(name = "attestrum", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Compile a corpus into a deterministic sealed manifest under
    /// `<workspace>/.attestrum/manifests/manifest.parquet`.
    Build {
        /// Path to corpus.toml describing the corpus to compile.
        #[arg(long, value_name = "PATH")]
        corpus: PathBuf,

        /// Workspace directory. The CAS lands at `<workspace>/.attestrum/`;
        /// the sealed manifest at
        /// `<workspace>/.attestrum/manifests/manifest.parquet`. Created if
        /// missing.
        #[arg(long, value_name = "PATH")]
        workspace: PathBuf,

        /// Reproducible Builds timestamp (epoch seconds). Overrides any
        /// `[corpus] source_date_epoch` in the toml; used as the default
        /// for entries that don't specify `fetched_at`.
        #[arg(long, value_name = "TS")]
        source_date_epoch: Option<i64>,

        /// Reserved for forward compatibility with the Sprint 4 fetch
        /// layer. No-op in v1 — `build` never makes network calls.
        #[arg(long)]
        offline: bool,
    },

    /// Read a sealed manifest and print a human-readable summary
    /// (Merkle root, leaf count, total bytes, per-modality histogram).
    /// Always offline; never mutates any file.
    Inspect {
        /// Path to the sealed `manifest.parquet`.
        #[arg(value_name = "MANIFEST")]
        manifest: PathBuf,

        /// Accepted for CLI-uniformity. `inspect` is always offline;
        /// the flag is a no-op.
        #[arg(long)]
        offline: bool,
    },

    /// Partition a corpus.toml into N shard files for parallel /
    /// distributed building. Deterministic shard assignment via
    /// `BLAKE3(source_url) mod N`; re-running with the same args
    /// produces byte-identical output.
    Plan {
        /// Path to the input corpus.toml.
        #[arg(long, value_name = "PATH")]
        corpus: PathBuf,

        /// Number of shards to partition into. Must be >= 1.
        #[arg(long, value_name = "N")]
        shards: u32,

        /// Output directory. Created if missing. One
        /// `shard-NNNN.toml` file per non-empty shard.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },

    /// Merge N sealed shard manifests into one. The merged Merkle
    /// root equals the root of an unsharded build of the same
    /// logical corpus.
    Merge {
        /// Input shard manifests. Pass via shell glob, e.g.
        /// `--inputs shards/shard-*.parquet`.
        #[arg(long = "inputs", value_name = "PATH", num_args = 1..)]
        inputs: Vec<PathBuf>,

        /// Output path for the merged `manifest.parquet`.
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
    },

    /// Verify a Sigstore Bundle v0.3 against a sealed manifest. Reads
    /// the bundle + manifest, refreshes the TUF trust root (unless
    /// `--offline`), cryptographically validates the cert chain + DSSE
    /// signature + Rekor inclusion proof + RFC3161 timestamp, checks
    /// the extracted identity against the operator-supplied regex
    /// policy, and lightweight-validates the in-toto predicate against
    /// the v0.3 training-corpus schema.
    Verify {
        /// Path to the Sigstore Bundle v0.3 JSON file.
        #[arg(value_name = "BUNDLE")]
        bundle: PathBuf,

        /// Path to the manifest file being attested (the in-toto subject).
        /// Required; auto-resolve from `bundle.subject[0].name` is a
        /// follow-up.
        #[arg(long, value_name = "PATH")]
        manifest: PathBuf,

        /// Regex (anchored) matched against the extracted SAN value
        /// (cosign-compatible `--certificate-identity-regexp` semantics).
        #[arg(long, value_name = "REGEX")]
        certificate_identity: String,

        /// Regex (anchored) matched against the extracted Fulcio
        /// OIDC-issuer extension.
        #[arg(long, value_name = "REGEX")]
        certificate_oidc_issuer: String,

        /// Skip online Rekor inclusion-proof re-check; trust the bundle's
        /// embedded signed inclusion promise + RFC3161 TSA timestamp
        /// against the cached trust root.
        #[arg(long)]
        offline: bool,

        /// Stream the validated in-toto predicate as canonical JSON to
        /// stdout after the success summary (pipeable into `jq`).
        #[arg(long)]
        print_predicate: bool,
    },

    /// Sign a sealed manifest into a Sigstore Bundle v0.3 carrying an
    /// in-toto v1 Statement with a training-corpus/v0.3 predicate.
    /// Networks (Fulcio + Rekor + TUF) + requires an OIDC id_token.
    Sign {
        /// Path to the sealed `manifest.parquet`.
        #[arg(value_name = "MANIFEST")]
        manifest: PathBuf,

        /// Workspace directory. The bundle lands at
        /// `<workspace>/bundles/<manifest-stem>.sigstore.json`. Default:
        /// `<cwd>/.attestrum/`.
        #[arg(long, value_name = "PATH")]
        workspace: Option<PathBuf>,

        /// Reproducible Builds timestamp (epoch seconds) — feeds the
        /// predicate's `built_at` + `determinism.seed` fields. Required
        /// either via this flag OR via the `SOURCE_DATE_EPOCH` env var
        /// (no wall-clock fallback per CLAUDE.md §7).
        #[arg(long, value_name = "TS")]
        source_date_epoch: Option<i64>,

        /// Read OIDC id_token (JWT) from this file. Takes precedence
        /// over the `SIGSTORE_ID_TOKEN` env var if both are set.
        #[arg(long, value_name = "PATH")]
        oidc_token_file: Option<PathBuf>,

        /// Exit 3 immediately. `attestrum sign` always requires network
        /// (Fulcio + Rekor + TUF); the flag is the documented
        /// "did-you-mean-this?" gate.
        #[arg(long)]
        offline: bool,

        /// Optional `takedown_contact` predicate field (mailto: URL).
        #[arg(long, value_name = "MAILTO")]
        takedown_contact: Option<String>,

        /// Optional `dataset_homepage` predicate field (URL).
        #[arg(long, value_name = "URL")]
        dataset_homepage: Option<String>,

        /// Optional `publication_intent` predicate field. One of:
        /// `hf` / `huggingface-hub`, `zenodo`, `github-release`,
        /// `eu-ai-office`, `private`.
        #[arg(long, value_name = "TARGET")]
        publication_intent: Option<String>,
    },
}

fn main() -> ExitCode {
    // Honour RUST_LOG when set; otherwise default to INFO so users see
    // basic progress without noise. `tracing-subscriber` writes to
    // stderr by default, keeping stdout reserved for the structured
    // summary that operators may want to pipe.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    // clap exits with code 2 on parse failure; we only see the parsed Cli.
    let cli = Cli::parse();
    match cli.command {
        Command::Build {
            corpus,
            workspace,
            source_date_epoch,
            offline,
        } => match commands::build::run(commands::build::Args {
            corpus,
            workspace,
            source_date_epoch,
            offline,
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("attestrum build: {err}");
                let mut source = std::error::Error::source(&err);
                while let Some(s) = source {
                    eprintln!("  caused by: {s}");
                    source = std::error::Error::source(s);
                }
                ExitCode::from(1)
            }
        },

        Command::Inspect { manifest, offline } => {
            let code = commands::inspect::run(commands::inspect::Args { manifest, offline });
            ExitCode::from(code)
        }

        Command::Plan {
            corpus,
            shards,
            out,
        } => {
            let code = commands::plan::run(commands::plan::Args {
                corpus,
                shards,
                out,
            });
            ExitCode::from(code)
        }

        Command::Merge { inputs, out } => {
            let code = commands::merge::run(commands::merge::Args { inputs, out });
            ExitCode::from(code)
        }

        Command::Verify {
            bundle,
            manifest,
            certificate_identity,
            certificate_oidc_issuer,
            offline,
            print_predicate,
        } => {
            let code = commands::verify::run(commands::verify::Args {
                bundle,
                manifest,
                certificate_identity,
                certificate_oidc_issuer,
                offline,
                print_predicate,
            });
            ExitCode::from(code)
        }

        Command::Sign {
            manifest,
            workspace,
            source_date_epoch,
            oidc_token_file,
            offline,
            takedown_contact,
            dataset_homepage,
            publication_intent,
        } => {
            let code = commands::sign::run(commands::sign::Args {
                manifest,
                workspace,
                source_date_epoch,
                oidc_token_file,
                offline,
                takedown_contact,
                dataset_homepage,
                publication_intent,
            });
            ExitCode::from(code)
        }
    }
}
