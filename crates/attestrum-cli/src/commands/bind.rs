//! `attestrum bind` — emit a `model-binding/v0.1` attestation.
//!
//! Binds a model (its weights-manifest file's digest) to one or more
//! training-corpus attestation bundles, each with a role. Default signs via
//! Sigstore (Fulcio + Rekor); `--unsigned` skips signing.
//!
//! OIDC id_token resolves via `--oidc-token-file` > `SIGSTORE_ID_TOKEN` env
//! (mirrors `attestrum sign`). `--source-date-epoch` is required (flag >
//! `SOURCE_DATE_EPOCH` env > error) per CLAUDE.md §7.
//!
//! Error → exit-code mapping (reuses [`ExitCode`]):
//!
//! | `BindError` variant                         | `ExitCode`     | Numeric |
//! |---------------------------------------------|----------------|---------|
//! | `Sign(_)`                                   | `IdentityError`| 4       |
//! | `Corpus`/`CorpusPredicate`/`Serialize`/`Canonicalize`/`Timestamp`/`Io` | `RuntimeError` | 1 |
//! | arg-parse failure                           | `ArgsError`    | 2       |

use std::path::{Path, PathBuf};

use attestrum_attest::{DigestMap, ModelRef};
use attestrum_bind::{bind as bind_lib, BindArtifact, BindError, BindOpts, BoundCorpus};
use sha2::{Digest as _, Sha256};

use crate::lifecycle::ExitCode;

#[derive(Debug)]
pub struct Args {
    /// `--model-card-uri URI` — the model identity / Statement subject name.
    pub model_card_uri: String,
    /// `--model-manifest PATH` — the model weights-manifest file. Its BLAKE3 +
    /// SHA-256 digest becomes the binding subject digest (and the model digest
    /// a later `walk-chain` asserts).
    pub model_manifest: PathBuf,
    /// `--corpus PATH` (repeatable) — training-corpus attestation bundles.
    pub corpus: Vec<PathBuf>,
    /// `--role ROLE` (repeatable, paired by position with `--corpus`).
    pub role: Vec<String>,
    /// `--builder-identity NAME` — human-readable claimant.
    pub builder_identity: String,
    /// `--signing-bundle-ref REF` — optional reference (URI/digest) to the
    /// model's own OpenSSF/Sigstore signing bundle (recorded, not verified).
    pub signing_bundle_ref: Option<String>,
    /// `--source-date-epoch SECS` (flag > `SOURCE_DATE_EPOCH` env > error).
    pub source_date_epoch: Option<i64>,
    /// `--workspace DIR` — signed-bundle output dir (default `<cwd>/.attestrum`).
    pub workspace: Option<PathBuf>,
    /// `--oidc-token-file PATH` — overrides `SIGSTORE_ID_TOKEN` when signing.
    pub oidc_token_file: Option<PathBuf>,
    /// `--unsigned` — skip Sigstore signing (default signs).
    pub unsigned: bool,
}

pub fn run(args: Args) -> u8 {
    if args.corpus.is_empty() || args.corpus.len() != args.role.len() {
        eprintln!(
            "attestrum bind: need an equal, non-zero number of --corpus and --role flags \
             (got {} corpora, {} roles)",
            args.corpus.len(),
            args.role.len()
        );
        return ExitCode::ArgsError.as_u8();
    }

    let source_date_epoch = match resolve_source_date_epoch(&args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("attestrum bind: {msg}");
            return ExitCode::ArgsError.as_u8();
        }
    };

    let model_digest = match hash_file(&args.model_manifest) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "attestrum bind: hashing model manifest {}: {e}",
                args.model_manifest.display()
            );
            return ExitCode::RuntimeError.as_u8();
        }
    };

    let sign = !args.unsigned;
    let oidc_id_token = if sign {
        match resolve_oidc_token(&args) {
            Ok(t) => Some(t),
            Err(msg) => {
                eprintln!("attestrum bind: {msg}");
                return ExitCode::IdentityError.as_u8();
            }
        }
    } else {
        None
    };

    let corpora = args
        .corpus
        .iter()
        .zip(args.role.iter())
        .map(|(p, r)| BoundCorpus {
            bundle_path: p.clone(),
            role: r.clone(),
        })
        .collect();

    let opts = BindOpts {
        model: ModelRef {
            identity: args.model_card_uri.clone(),
            weights_manifest_digest: model_digest,
            signing_bundle_ref: args.signing_bundle_ref.clone(),
        },
        model_card_uri: args.model_card_uri.clone(),
        corpora,
        builder_identity: args.builder_identity.clone(),
        config_digest: None,
        source_date_epoch,
        builder_version: concat!("attestrum-cli/", env!("CARGO_PKG_VERSION")).to_string(),
        sign,
        oidc_id_token,
        workspace: args.workspace.clone(),
    };

    match bind_lib(&opts) {
        Ok(artifact) => {
            print_summary(&args, &artifact);
            ExitCode::Ok.as_u8()
        }
        Err(err) => {
            eprintln!("attestrum bind: {err}");
            map_error_to_exit_code(&err).as_u8()
        }
    }
}

fn resolve_source_date_epoch(args: &Args) -> Result<i64, String> {
    if let Some(s) = args.source_date_epoch {
        return Ok(s);
    }
    if let Ok(s) = std::env::var("SOURCE_DATE_EPOCH") {
        return s
            .parse::<i64>()
            .map_err(|e| format!("SOURCE_DATE_EPOCH env var is not a valid integer: {s:?} ({e})"));
    }
    Err("required: pass --source-date-epoch <SECS> or set SOURCE_DATE_EPOCH env var".to_string())
}

fn resolve_oidc_token(args: &Args) -> Result<String, String> {
    if let Some(path) = &args.oidc_token_file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read --oidc-token-file {}: {e}", path.display()))?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(format!(
                "--oidc-token-file {} is empty after trim",
                path.display()
            ));
        }
        return Ok(trimmed);
    }
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(s) if !s.is_empty() => Ok(s),
        _ => Err(
            "OIDC id_token required to sign: pass --oidc-token-file <PATH>, set \
                  SIGSTORE_ID_TOKEN env var, or pass --unsigned"
                .to_string(),
        ),
    }
}

/// BLAKE3 + SHA-256 of a file's bytes (mirrors `attestrum sign`).
fn hash_file(path: &Path) -> std::io::Result<DigestMap> {
    let bytes = std::fs::read(path)?;
    let blake3 = attestrum_core::hex::encode_32(blake3::hash(&bytes).as_bytes());
    let sha256_bytes: [u8; 32] = Sha256::digest(&bytes).into();
    let sha256 = attestrum_core::hex::encode_32(&sha256_bytes);
    Ok(DigestMap { blake3, sha256 })
}

fn map_error_to_exit_code(err: &BindError) -> ExitCode {
    match err {
        BindError::Sign(_) => ExitCode::IdentityError,
        BindError::Corpus(_)
        | BindError::CorpusPredicate(_)
        | BindError::Serialize(_)
        | BindError::Canonicalize(_)
        | BindError::Timestamp(_)
        | BindError::Io(_) => ExitCode::RuntimeError,
    }
}

fn print_summary(args: &Args, artifact: &BindArtifact) {
    println!("binding:         {}", artifact.statement.predicate_type);
    println!("model:           {}", args.model_card_uri);
    println!("corpora:         {}", args.corpus.len());
    match &artifact.bundle_path {
        Some(p) => println!("bundle:          {}", p.display()),
        None => println!("bundle:          (unsigned)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(corpus: Vec<&str>, role: Vec<&str>) -> Args {
        Args {
            model_card_uri: "https://hf.co/acme/m".to_string(),
            model_manifest: PathBuf::from("model.json"),
            corpus: corpus.into_iter().map(PathBuf::from).collect(),
            role: role.into_iter().map(String::from).collect(),
            builder_identity: "Acme".to_string(),
            signing_bundle_ref: None,
            source_date_epoch: Some(1_700_000_000),
            workspace: None,
            oidc_token_file: None,
            unsigned: true,
        }
    }

    #[test]
    fn mismatched_corpus_role_counts_are_args_error() {
        let code = run(args(vec!["a.json", "b.json"], vec!["pretraining"]));
        assert_eq!(code, ExitCode::ArgsError.as_u8());
    }

    #[test]
    fn empty_corpora_is_args_error() {
        let code = run(args(vec![], vec![]));
        assert_eq!(code, ExitCode::ArgsError.as_u8());
    }

    #[test]
    fn error_mapping_covers_all_bind_error_variants() {
        // Compile-time exhaustiveness: a new BindError variant without a CLI
        // mapping fails to compile here.
        assert_eq!(
            map_error_to_exit_code(&BindError::Timestamp("x".into())).as_u8(),
            ExitCode::RuntimeError.as_u8()
        );
    }
}
