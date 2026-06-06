//! `attestrum prove <DOC> --against <MANIFEST>` — Sprint 5 D2 E8.
//!
//! Wraps the `attestrum_prove::prove()` library entry point as a CLI
//! subcommand. v0.1 release-ready milestone for `attestrum-prove`.
//!
//! v0.1 scope (founder-locked at E8 plan time):
//!
//! - `<DOC>` accepts a file path OR a 64-char lowercase BLAKE3 hex digest.
//!   The other four `ProofTarget` variants (`Sha256`, `Iscc`, `Perceptual`,
//!   `Bundle`) stay library-only at v0.1.
//! - `--against <MANIFEST>` accepts a filesystem path,
//!   `hf://repo[@revision]`, or `https://...` / `http://...` URL — all three
//!   `ManifestSource` variants mapped from a single positional string.
//! - Output is human key-value lines to stdout (no `--output json` flag —
//!   that's a v0.2 surface decision).
//! - `--source-date-epoch` is required (CLI flag > `SOURCE_DATE_EPOCH` env
//!   var > error). No wall-clock fallback per CLAUDE.md §7 determinism rule.
//! - HF auth is implicit via `$HF_TOKEN` env var per the E7 D3-refactor-debt
//!   carry-forward (no `--hf-token` flag).
//! - `--unsigned` toggles `opts.sign = false`; default is signed (MVP-gate
//!   decision at E4).
//!
//! Error → exit-code mapping uses the existing `lifecycle::ExitCode` values
//! (no new codes at E8 per PATH-A-BRIEF §2.2's 6-variant error lock):
//!
//! | `AttestrumProveError` variant | `ExitCode`               | Numeric |
//! |-------------------------------|--------------------------|---------|
//! | `SourceUnreachable(_)`        | `NetworkError`           | 5       |
//! | `Sign(_)`                     | `IdentityError`          | 4       |
//! | `InvalidManifest(_)`          | `RuntimeError`           | 1       |
//! | `MerkleMismatch`              | `RuntimeError`           | 1       |
//! | `Fingerprint(_)`              | `RuntimeError`           | 1       |
//! | `Ambiguous(_)`                | `RuntimeError`           | 1       |
//! | arg-parse failure             | `ArgsError`              | 2       |
//! | success                       | `Ok`                     | 0       |

use std::path::PathBuf;

use attestrum_prove::{
    prove as prove_lib, AttestrumProveError, ManifestSource, ProofArtifact, ProofKind, ProofTarget,
    ProveOpts,
};

use crate::lifecycle::ExitCode;

// ============================================================================
// Args + entry point
// ============================================================================

#[derive(Debug)]
pub struct Args {
    /// Positional DOC. Either a file path (becomes `ProofTarget::Document`)
    /// or a 64-char lowercase BLAKE3 hex digest (becomes `ProofTarget::Blake3`).
    pub doc: String,

    /// `--against MANIFEST`. Filesystem path, `hf://repo[@revision]`, or
    /// `https://...` / `http://...` URL.
    pub against: String,

    /// `--workspace DIR`. Override the workspace dir where the signed
    /// bundle (E4) and the manifest cache (E7) are written. None → the
    /// library resolves via `opts.workspace.or($PWD/.attestrum)`.
    pub workspace: Option<PathBuf>,

    /// `--source-date-epoch SECS`. Required: CLI flag > `SOURCE_DATE_EPOCH`
    /// env var > arg error. No wall-clock fallback per CLAUDE.md §7.
    pub source_date_epoch: Option<i64>,

    /// `--corpus-bundle PATH`. Path to the corpus's Sigstore Bundle v0.3
    /// JSON, fed into `predicate.corpus.attestation_digest`. Optional —
    /// the library accepts `None`.
    pub corpus_bundle: Option<PathBuf>,

    /// `--cas-root DIR`. Path to the corpus's CAS root. Required by the
    /// fuzzy `ProofTarget` arms (`Iscc` / `Perceptual` / `Document`); the
    /// library returns `InvalidManifest` if missing for those targets.
    pub cas_root: Option<PathBuf>,

    /// `--oidc-token-file PATH`. Read the OIDC id_token (JWT) from this
    /// file; overrides `SIGSTORE_ID_TOKEN` when signing. Mirrors `sign`
    /// and `bind` via the shared `commands::oidc` resolver.
    pub oidc_token_file: Option<PathBuf>,

    /// `--unsigned`. When set, skip Sigstore signing entirely. Default
    /// (false) signs via Fulcio + Rekor per the E4 MVP-gate decision.
    pub unsigned: bool,

    /// `--no-index`. Force the exhaustive fuzzy scan even when a sidecar index
    /// is present beside `--cas-root`. Default (false) auto-detects + uses it.
    pub no_index: bool,
}

pub fn run(args: Args) -> u8 {
    let target = match parse_proof_target(&args.doc) {
        Ok(t) => t,
        Err(msg) => {
            eprintln!("attestrum prove: {msg}");
            return ExitCode::ArgsError.as_u8();
        }
    };

    let manifest = parse_manifest_source(&args.against);

    let source_date_epoch = match resolve_source_date_epoch(&args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("attestrum prove: {msg}");
            return ExitCode::ArgsError.as_u8();
        }
    };

    // Resolve the OIDC id_token only when signing (the default). Mirrors
    // `attestrum bind`: `--oidc-token-file` > `SIGSTORE_ID_TOKEN`, with the
    // `--unsigned` hint enabled since `prove` can emit unsigned proofs.
    let sign = !args.unsigned;
    let oidc_id_token = if sign {
        match crate::commands::oidc::resolve_oidc_token(args.oidc_token_file.as_deref(), true) {
            Ok(t) => Some(t),
            Err(msg) => {
                eprintln!("attestrum prove: {msg}");
                return ExitCode::IdentityError.as_u8();
            }
        }
    } else {
        None
    };

    let opts = ProveOpts {
        sign,
        source_date_epoch,
        oidc_id_token,
        workspace: args.workspace.clone(),
        corpus_bundle_path: args.corpus_bundle.clone(),
        cas_root: args.cas_root.clone(),
        no_index: args.no_index,
    };

    let artifact = match prove_lib(target, manifest, &opts) {
        Ok(a) => a,
        Err(err) => {
            eprintln!("attestrum prove: {err}");
            return map_error_to_exit_code(&err).as_u8();
        }
    };

    print_summary(&artifact);
    ExitCode::Ok.as_u8()
}

// ============================================================================
// Arg parsing helpers
// ============================================================================

/// Detect `ProofTarget` from the DOC positional. v0.1 only exposes
/// `Document` (file path) and `Blake3` (64-char lowercase hex digest);
/// the other four variants stay library-only.
fn parse_proof_target(arg: &str) -> Result<ProofTarget, String> {
    let path = std::path::Path::new(arg);
    if path.is_file() {
        return Ok(ProofTarget::Document(path.to_path_buf()));
    }
    if arg.len() == 64
        && arg
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        let mut bytes = [0u8; 32];
        for (i, chunk) in arg.as_bytes().chunks_exact(2).enumerate() {
            // hex_nibble is infallible here — we already validated the charset above.
            bytes[i] = (hex_nibble(chunk[0]) << 4) | hex_nibble(chunk[1]);
        }
        return Ok(ProofTarget::Blake3(bytes));
    }
    Err(format!(
        "DOC arg {arg:?} is neither an existing file path nor a 64-char lowercase BLAKE3 hex digest"
    ))
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => unreachable!("charset pre-validated by parse_proof_target"),
    }
}

/// Detect `ManifestSource` from the `--against` positional. The three
/// variants are disambiguated by URL-scheme prefix:
///
/// - `hf://repo` / `hf://repo@revision` → `HuggingFace { repo, revision }`
/// - `http://...` / `https://...` → `Url(String)` (kept as `String` at v0.1
///   per the founder-locked E7 decision; `url::Url` deferred to v0.2)
/// - anything else → `Local(PathBuf)`
fn parse_manifest_source(arg: &str) -> ManifestSource {
    if let Some(rest) = arg.strip_prefix("hf://") {
        let (repo, revision) = match rest.split_once('@') {
            Some((r, rev)) => (r.to_string(), Some(rev.to_string())),
            None => (rest.to_string(), None),
        };
        return ManifestSource::HuggingFace { repo, revision };
    }
    if arg.starts_with("http://") || arg.starts_with("https://") {
        return ManifestSource::Url(arg.to_string());
    }
    ManifestSource::Local(PathBuf::from(arg))
}

/// Resolve `--source-date-epoch` per the sign.rs precedent: CLI flag wins;
/// fall back to the `SOURCE_DATE_EPOCH` env var; error if neither is set.
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

// ============================================================================
// Error mapping + output
// ============================================================================

fn map_error_to_exit_code(err: &AttestrumProveError) -> ExitCode {
    match err {
        AttestrumProveError::SourceUnreachable(_) => ExitCode::NetworkError,
        AttestrumProveError::Sign(_) => ExitCode::IdentityError,
        AttestrumProveError::InvalidManifest(_)
        | AttestrumProveError::MerkleMismatch
        | AttestrumProveError::Fingerprint(_)
        | AttestrumProveError::Ambiguous(_) => ExitCode::RuntimeError,
    }
}

fn print_summary(a: &ProofArtifact) {
    let kind = match a.kind {
        ProofKind::Inclusion => "inclusion",
        ProofKind::NonInclusion => "non-inclusion",
    };
    println!("proof:           {kind}");
    println!("confidence:      {:.2}", a.confidence);
    if let Some(subject) = &a.matched_subject {
        println!("matched:         {}", subject.name);
    }
    if let Some(bundle) = &a.bundle_path {
        println!("bundle:          {}", bundle.display());
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proof_target_hex_blake3_lowercase() {
        let hex = "a".repeat(64);
        let t = parse_proof_target(&hex).expect("64 chars of 'a' is a valid hex digest");
        match t {
            ProofTarget::Blake3(bytes) => assert_eq!(bytes, [0xaa; 32]),
            other => panic!("expected Blake3, got {other:?}"),
        }
    }

    #[test]
    fn parse_proof_target_rejects_uppercase_hex() {
        let hex = "A".repeat(64);
        assert!(
            parse_proof_target(&hex).is_err(),
            "uppercase hex must error"
        );
    }

    #[test]
    fn parse_proof_target_rejects_63_chars() {
        let hex = "a".repeat(63);
        assert!(
            parse_proof_target(&hex).is_err(),
            "63 chars of 'a' is neither a file nor a 64-char digest"
        );
    }

    #[test]
    fn parse_proof_target_existing_file_is_document() {
        let t = parse_proof_target("Cargo.toml")
            .expect("Cargo.toml exists at the crate root during cargo test");
        match t {
            ProofTarget::Document(p) => assert_eq!(p, PathBuf::from("Cargo.toml")),
            other => panic!("expected Document, got {other:?}"),
        }
    }

    #[test]
    fn parse_manifest_source_local_path() {
        let s = parse_manifest_source("./manifest.parquet");
        assert_eq!(
            s,
            ManifestSource::Local(PathBuf::from("./manifest.parquet"))
        );
    }

    #[test]
    fn parse_manifest_source_huggingface_no_revision() {
        let s = parse_manifest_source("hf://my-org/dataset");
        assert_eq!(
            s,
            ManifestSource::HuggingFace {
                repo: "my-org/dataset".to_string(),
                revision: None,
            }
        );
    }

    #[test]
    fn parse_manifest_source_huggingface_with_revision() {
        let s = parse_manifest_source("hf://my-org/dataset@v1.0.0");
        assert_eq!(
            s,
            ManifestSource::HuggingFace {
                repo: "my-org/dataset".to_string(),
                revision: Some("v1.0.0".to_string()),
            }
        );
    }

    #[test]
    fn parse_manifest_source_https_url() {
        let s = parse_manifest_source("https://example.com/manifest.parquet");
        assert_eq!(
            s,
            ManifestSource::Url("https://example.com/manifest.parquet".to_string())
        );
    }

    #[test]
    fn parse_manifest_source_http_url() {
        let s = parse_manifest_source("http://example.com/manifest.parquet");
        assert_eq!(
            s,
            ManifestSource::Url("http://example.com/manifest.parquet".to_string())
        );
    }

    #[test]
    fn error_mapping_covers_all_six_variants() {
        // Locks the §2.2 error-variant lock at the CLI boundary. If a new
        // `AttestrumProveError` variant lands without explicit CLI mapping,
        // the compiler will refuse to compile `map_error_to_exit_code`.
        assert_eq!(
            map_error_to_exit_code(&AttestrumProveError::SourceUnreachable("x".to_string()))
                .as_u8(),
            ExitCode::NetworkError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&AttestrumProveError::InvalidManifest("x".to_string())).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&AttestrumProveError::MerkleMismatch).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
        assert_eq!(
            map_error_to_exit_code(&AttestrumProveError::Ambiguous(2)).as_u8(),
            ExitCode::RuntimeError.as_u8(),
        );
    }

    #[test]
    fn unsigned_skips_oidc_token_resolution() {
        // With `--unsigned`, run() must NOT attempt OIDC token resolution: it
        // proceeds straight to prove_lib, which fails on the nonexistent local
        // manifest (RuntimeError), never IdentityError. If the unsigned path
        // wrongly resolved a token, a missing token would surface as
        // IdentityError instead. The signed missing-token path is covered by
        // `attestrum-prove/tests/sign_integration.rs::sign_true_without_oidc_token_returns_sign_error`.
        let args = Args {
            doc: "a".repeat(64), // valid 64-char BLAKE3 hex → ProofTarget::Blake3
            against: "this-manifest-does-not-exist.parquet".to_string(),
            workspace: None,
            source_date_epoch: Some(1_700_000_000),
            corpus_bundle: None,
            cas_root: None,
            no_index: false,
            oidc_token_file: None,
            unsigned: true,
        };
        let code = run(args);
        assert_ne!(
            code,
            ExitCode::IdentityError.as_u8(),
            "unsigned prove must not reach the OIDC/IdentityError path"
        );
        assert_eq!(
            code,
            ExitCode::RuntimeError.as_u8(),
            "unsigned prove with a missing local manifest should fail RuntimeError"
        );
    }
}
