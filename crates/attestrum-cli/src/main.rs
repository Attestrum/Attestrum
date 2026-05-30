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

    /// Prove that a document is (or isn't) part of a corpus manifest.
    /// Reads the manifest (local, hf://, or https://), matches the
    /// document via exact-hash or fuzzy modality, and emits an in-toto
    /// inclusion-proof or non-inclusion-proof attestation. Default is
    /// signed via Fulcio + Rekor (Sigstore Bundle v0.3); pass `--unsigned`
    /// to skip signing. HuggingFace private datasets require the
    /// `HF_TOKEN` env var.
    Prove {
        /// Document to prove. Either a file path (becomes
        /// `ProofTarget::Document`) or a 64-char lowercase BLAKE3 hex
        /// digest (becomes `ProofTarget::Blake3`).
        #[arg(value_name = "DOC")]
        doc: String,

        /// Manifest source. Three forms:
        ///   - filesystem path (e.g. `./manifest.parquet`)
        ///   - `hf://repo[@revision]` (HuggingFace dataset)
        ///   - `https://...` or `http://...` (arbitrary URL)
        #[arg(long, value_name = "MANIFEST")]
        against: String,

        /// Workspace dir for the signed bundle (E4) and manifest cache
        /// (E7). Default: `<cwd>/.attestrum/`.
        #[arg(long, value_name = "PATH")]
        workspace: Option<PathBuf>,

        /// Reproducible Builds timestamp (epoch seconds) — feeds the
        /// proof's `proof_generated_at` field. Required either via this
        /// flag OR via the `SOURCE_DATE_EPOCH` env var (no wall-clock
        /// fallback per CLAUDE.md §7).
        #[arg(long, value_name = "TS")]
        source_date_epoch: Option<i64>,

        /// Path to the corpus's Sigstore Bundle v0.3 JSON. Fed into
        /// `predicate.corpus.attestation_digest`. Optional.
        #[arg(long, value_name = "PATH")]
        corpus_bundle: Option<PathBuf>,

        /// Path to the corpus's CAS root (typically `<corpus>/.attestrum/`).
        /// Required by the fuzzy `Document` proof target so the prover
        /// can re-fingerprint the document against the corpus's stored
        /// inline fingerprints.
        #[arg(long, value_name = "DIR")]
        cas_root: Option<PathBuf>,

        /// Skip Sigstore signing. Default is signed via Fulcio + Rekor
        /// (the E4 MVP-gate decision). Unsigned proofs are still
        /// cryptographically self-contained — they just don't carry a
        /// signing-identity attestation.
        #[arg(long)]
        unsigned: bool,
    },

    /// Publish a signed bundle + sealed manifest + dataset card to
    /// Hugging Face Hub. Constructs the three dataset-side artifacts
    /// (Croissant JSON-LD, dataset card README, verify.html stub) and
    /// commits them alongside the manifest, Merkle root, and Sigstore
    /// bundle into the target dataset repo. HF auth resolves via the
    /// `hf-hub` token chain (`HF_TOKEN` env → `HF_TOKEN_PATH` file →
    /// `$HF_HOME/token`); private datasets require a token.
    Publish {
        /// Hugging Face dataset repo in `ORG/NAME` shape (e.g.
        /// `my-org/my-dataset`). Personal accounts and orgs are accepted
        /// identically.
        #[arg(long, value_name = "ORG/NAME")]
        dataset: String,

        /// Path to the sealed `manifest.parquet` (output of
        /// `attestrum build`).
        #[arg(long, value_name = "PATH")]
        manifest: PathBuf,

        /// Path to the Sigstore Bundle v0.3 JSON (output of
        /// `attestrum sign`).
        #[arg(long, value_name = "PATH")]
        bundle: PathBuf,

        /// Reproducible Builds timestamp (epoch seconds) — feeds the
        /// Croissant emitter's `dateCreated` field. Required either via
        /// this flag OR via the `SOURCE_DATE_EPOCH` env var (no
        /// wall-clock fallback per CLAUDE.md §7).
        #[arg(long, value_name = "TS")]
        source_date_epoch: Option<i64>,

        /// SPDX license identifier for the corpus (e.g. `Apache-2.0`,
        /// `CC-BY-4.0`, or `mixed`). Threaded into both the Croissant
        /// `license` field and the dataset-card README. When omitted, the
        /// artifacts record the honest token `unknown` (a value both
        /// mlcroissant and the HF Hub accept) rather than asserting a
        /// license the publisher didn't declare.
        #[arg(long, value_name = "SPDX")]
        license: Option<String>,

        /// Semver dataset version for the Croissant `version` field (e.g.
        /// `1.0.0`). Defaults to `1.0.0` (the first sealed release).
        /// mlcroissant requires MAJOR.MINOR.PATCH and warns otherwise.
        #[arg(long, value_name = "SEMVER")]
        version: Option<String>,

        /// Citation string for the Croissant `citeAs` field (BibTeX or
        /// prose). Omitted when not supplied — never synthesized.
        #[arg(long, value_name = "TEXT")]
        cite_as: Option<String>,

        /// Publish target. `huggingface` pushes to the HF Hub; `static`
        /// writes the same artifact set to a local `--out-dir` (no network;
        /// upload to Zenodo / GitHub Pages / S3 / any static host).
        /// `github-release` is a v0.2 deferral that returns
        /// `NotImplemented` → exit 1.
        #[arg(long, value_name = "TARGET", default_value = "huggingface")]
        target: String,

        /// HF branch to commit against. Default: `main`.
        #[arg(long, value_name = "REV", default_value = "main")]
        revision: String,

        /// Workspace dir used to locate the default Merkle-root path.
        /// Default: `./.attestrum/`.
        #[arg(long, value_name = "DIR")]
        workspace: Option<PathBuf>,

        /// Override the default Merkle-root file path. Default:
        /// `<workspace>/.attestrum/manifests/merkle.root` (matches where
        /// `attestrum build` writes the file).
        #[arg(long, value_name = "PATH")]
        merkle_root: Option<PathBuf>,

        /// Optional path to a file holding the HF Hub token (overrides
        /// `HF_TOKEN_PATH`).
        #[arg(long, value_name = "PATH")]
        token_file: Option<PathBuf>,

        /// Skip GPG-signing git commits on the Hub. Default: sign if a
        /// gpg key is available.
        #[arg(long)]
        no_sign_commits: bool,

        /// Output directory. Only consulted by `--target static`.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
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

    /// Bind a model to its training corpora — emit a `model-binding/v0.1`
    /// in-toto attestation (model as subject, corpus attestations as
    /// materials). Default signs via Fulcio + Rekor; `--unsigned` skips it.
    /// OIDC resolves via `--oidc-token-file` > `SIGSTORE_ID_TOKEN` env.
    Bind {
        /// Model identity / Statement subject name (model-card or release URI).
        #[arg(long, value_name = "URI")]
        model_card_uri: String,

        /// The model weights-manifest file. Its BLAKE3 + SHA-256 digest becomes
        /// the binding subject digest.
        #[arg(long, value_name = "PATH")]
        model_manifest: PathBuf,

        /// A training-corpus attestation bundle (repeatable). Pair each with a
        /// `--role` in the same order.
        #[arg(long = "corpus", value_name = "PATH")]
        corpus: Vec<PathBuf>,

        /// The role a corpus played (repeatable, paired by position with
        /// `--corpus`): e.g. `pretraining`, `finetuning`, `rlhf`.
        #[arg(long = "role", value_name = "ROLE")]
        role: Vec<String>,

        /// Human-readable claimant identity.
        #[arg(long, value_name = "NAME")]
        builder_identity: String,

        /// Optional reference (URI/digest) to the model's own OpenSSF/Sigstore
        /// signing bundle (recorded, not verified at v0.1).
        #[arg(long, value_name = "REF")]
        signing_bundle_ref: Option<String>,

        /// Reproducible Builds timestamp (epoch seconds). Required via this
        /// flag OR the `SOURCE_DATE_EPOCH` env var (no wall-clock fallback).
        #[arg(long, value_name = "TS")]
        source_date_epoch: Option<i64>,

        /// Workspace dir for the signed bundle. Default: `<cwd>/.attestrum/`.
        #[arg(long, value_name = "PATH")]
        workspace: Option<PathBuf>,

        /// Read OIDC id_token (JWT) from this file. Overrides
        /// `SIGSTORE_ID_TOKEN` when signing.
        #[arg(long, value_name = "PATH")]
        oidc_token_file: Option<PathBuf>,

        /// Skip Sigstore signing. Default signs via Fulcio + Rekor.
        #[arg(long)]
        unsigned: bool,
    },

    /// Walk the corpus-to-model binding chain — verify "is work X in the
    /// corpus that trained model M?". Sigstore-verifies the binding and each
    /// corpus bundle, then re-runs the membership proof live. Multi-corpus
    /// results are OR-ed ("in at least one corpus that trained M").
    WalkChain {
        /// The model weights-manifest file — the binding bundle's subject; its
        /// digest is the model digest the binding must carry.
        #[arg(long, value_name = "PATH")]
        model_manifest: PathBuf,

        /// The signed `model-binding/v0.1` bundle.
        #[arg(long, value_name = "PATH")]
        binding: PathBuf,

        /// A signed training-corpus bundle (repeatable). Pair each with a
        /// `--corpus-manifest` in the same order.
        #[arg(long = "corpus-bundle", value_name = "PATH")]
        corpus_bundle: Vec<PathBuf>,

        /// The local corpus manifest a bundle attests (repeatable, paired by
        /// position with `--corpus-bundle`).
        #[arg(long = "corpus-manifest", value_name = "PATH")]
        corpus_manifest: Vec<PathBuf>,

        /// The work X to test for membership. A file (BLAKE3-hashed) or a
        /// 64-char lowercase BLAKE3 hex digest.
        #[arg(long, value_name = "DOC")]
        doc: String,

        /// Anchored regex matched against the bundles' SAN (both bundles).
        #[arg(long, value_name = "REGEX")]
        certificate_identity: String,

        /// Anchored regex matched against the bundles' OIDC issuer.
        #[arg(long, value_name = "REGEX")]
        certificate_oidc_issuer: String,

        /// Skip the online Rekor inclusion re-check.
        #[arg(long)]
        offline: bool,
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

        Command::Prove {
            doc,
            against,
            workspace,
            source_date_epoch,
            corpus_bundle,
            cas_root,
            unsigned,
        } => {
            let code = commands::prove::run(commands::prove::Args {
                doc,
                against,
                workspace,
                source_date_epoch,
                corpus_bundle,
                cas_root,
                unsigned,
            });
            ExitCode::from(code)
        }

        Command::Publish {
            dataset,
            manifest,
            bundle,
            source_date_epoch,
            license,
            version,
            cite_as,
            target,
            revision,
            workspace,
            merkle_root,
            token_file,
            no_sign_commits,
            out_dir,
        } => {
            // Parse --target into the enum here so a bad value exits 2
            // before the run() helper is invoked.
            let target_kind = match commands::publish::TargetKind::parse(&target) {
                Ok(k) => k,
                Err(msg) => {
                    eprintln!("attestrum publish: {msg}");
                    return ExitCode::from(2);
                }
            };
            let code = commands::publish::run(commands::publish::Args {
                dataset,
                manifest,
                bundle,
                source_date_epoch,
                license,
                version,
                cite_as,
                target: target_kind,
                revision,
                workspace,
                merkle_root,
                token_file,
                no_sign_commits,
                out_dir,
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

        Command::Bind {
            model_card_uri,
            model_manifest,
            corpus,
            role,
            builder_identity,
            signing_bundle_ref,
            source_date_epoch,
            workspace,
            oidc_token_file,
            unsigned,
        } => {
            let code = commands::bind::run(commands::bind::Args {
                model_card_uri,
                model_manifest,
                corpus,
                role,
                builder_identity,
                signing_bundle_ref,
                source_date_epoch,
                workspace,
                oidc_token_file,
                unsigned,
            });
            ExitCode::from(code)
        }

        Command::WalkChain {
            model_manifest,
            binding,
            corpus_bundle,
            corpus_manifest,
            doc,
            certificate_identity,
            certificate_oidc_issuer,
            offline,
        } => {
            let code = commands::walk_chain::run(commands::walk_chain::Args {
                model_manifest,
                binding,
                corpus_bundle,
                corpus_manifest,
                doc,
                certificate_identity,
                certificate_oidc_issuer,
                offline,
            });
            ExitCode::from(code)
        }
    }
}
