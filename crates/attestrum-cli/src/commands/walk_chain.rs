//! `attestrum walk-chain` — verify the corpus-to-model binding chain.
//!
//! Answers "is work X in the corpus that trained model M?" by Sigstore-verifying
//! the binding bundle and each corpus bundle, then re-running the membership
//! proof live. Multi-corpus: pass repeated `--corpus-bundle` / `--corpus-manifest`
//! pairs; the result is OR-ed ("X in **at least one** corpus that trained M").
//!
//! Exit codes (reuse [`ExitCode`]):
//!
//! - `0` — the chain walked: X is in ≥1 bound corpus, OR X is definitively in
//!   none (and every corpus verified cleanly).
//! - `2` — arg error (paired flag-count mismatch, unparseable `--doc`).
//! - `6` — verification failure: no corpus proved inclusion AND at least one
//!   corpus failed to verify/link, so "not in any" cannot be soundly asserted.

use std::path::{Path, PathBuf};

use attestrum_attest::DigestMap;
use attestrum_bind::{
    walk_chain as walk_chain_lib, BindingInput, ChainWalkOutcome, CorpusInput, IdentityPolicy,
};
use attestrum_prove::ProofTarget;
use sha2::{Digest as _, Sha256};

use crate::lifecycle::ExitCode;

#[derive(Debug)]
pub struct Args {
    /// `--model-manifest PATH` — the model weights-manifest file; its digest is
    /// the model digest the binding subject must carry, and the file verify()
    /// checks the binding bundle's subject against.
    pub model_manifest: PathBuf,
    /// `--binding PATH` — the signed `model-binding/v0.1` bundle.
    pub binding: PathBuf,
    /// `--corpus-bundle PATH` (repeatable) — signed training-corpus bundles.
    pub corpus_bundle: Vec<PathBuf>,
    /// `--corpus-manifest PATH` (repeatable, paired by position) — the local
    /// corpus manifest each bundle attests.
    pub corpus_manifest: Vec<PathBuf>,
    /// `--doc DOC` — the work X. A file (BLAKE3-hashed to an exact target) or a
    /// 64-char lowercase BLAKE3 hex digest.
    pub doc: String,
    /// `--certificate-identity REGEX` — anchored SAN policy (both bundles).
    pub certificate_identity: String,
    /// `--certificate-oidc-issuer REGEX` — anchored issuer policy (both bundles).
    pub certificate_oidc_issuer: String,
    /// `--offline` — skip the online Rekor re-check.
    pub offline: bool,
}

pub fn run(args: Args) -> u8 {
    if args.corpus_bundle.is_empty() || args.corpus_bundle.len() != args.corpus_manifest.len() {
        eprintln!(
            "attestrum walk-chain: need an equal, non-zero number of --corpus-bundle and \
             --corpus-manifest flags (got {} bundles, {} manifests)",
            args.corpus_bundle.len(),
            args.corpus_manifest.len()
        );
        return ExitCode::ArgsError.as_u8();
    }

    let model_digest = match hash_file(&args.model_manifest) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "attestrum walk-chain: hashing model manifest {}: {e}",
                args.model_manifest.display()
            );
            return ExitCode::RuntimeError.as_u8();
        }
    };

    let query = match parse_query(&args.doc) {
        Ok(q) => q,
        Err(msg) => {
            eprintln!("attestrum walk-chain: {msg}");
            return ExitCode::ArgsError.as_u8();
        }
    };

    let policy = IdentityPolicy {
        identity_regex: &args.certificate_identity,
        issuer_regex: &args.certificate_oidc_issuer,
        offline: args.offline,
    };

    let mut in_corpus = false;
    let mut any_error = false;
    for (bundle, manifest) in args.corpus_bundle.iter().zip(args.corpus_manifest.iter()) {
        let outcome = walk_chain_lib(
            &model_digest,
            BindingInput {
                bundle_path: &args.binding,
                manifest_path: &args.model_manifest,
                policy,
            },
            CorpusInput {
                bundle_path: bundle,
                manifest_path: manifest,
                policy,
            },
            query.clone(),
        );
        match outcome {
            Ok(ChainWalkOutcome::InCorpus { role, .. }) => {
                in_corpus = true;
                println!("corpus {}: IN  (role: {role})", bundle.display());
            }
            Ok(ChainWalkOutcome::NotInCorpus { .. }) => {
                println!("corpus {}: not in", bundle.display());
            }
            Err(err) => {
                any_error = true;
                eprintln!("corpus {}: ERROR — {err}", bundle.display());
            }
        }
    }

    if in_corpus {
        // OR-semantics: a single inclusion answers the membership query.
        println!("result:          X IS in at least one corpus that trained the model");
        ExitCode::Ok.as_u8()
    } else if any_error {
        // No inclusion AND some corpus did not verify → cannot soundly assert
        // "not in any".
        println!("result:          UNDETERMINED (a bound corpus failed to verify)");
        ExitCode::VerificationFailure.as_u8()
    } else {
        println!("result:          X is NOT in any corpus that trained the model");
        ExitCode::Ok.as_u8()
    }
}

/// Parse `--doc` into an **exact** [`ProofTarget`] (walk-chain does not thread a
/// CAS root, so the fuzzy arms are unavailable): an existing file is
/// BLAKE3-hashed; a 64-char lowercase hex string is taken as a BLAKE3 digest.
fn parse_query(arg: &str) -> Result<ProofTarget, String> {
    let path = Path::new(arg);
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|e| format!("reading --doc {arg:?}: {e}"))?;
        return Ok(ProofTarget::Blake3(*blake3::hash(&bytes).as_bytes()));
    }
    if arg.len() == 64
        && arg
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        let mut out = [0u8; 32];
        for (i, chunk) in arg.as_bytes().chunks_exact(2).enumerate() {
            out[i] = (hex_nibble(chunk[0]) << 4) | hex_nibble(chunk[1]);
        }
        return Ok(ProofTarget::Blake3(out));
    }
    Err(format!(
        "--doc {arg:?} is neither an existing file nor a 64-char lowercase BLAKE3 hex digest"
    ))
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => unreachable!("charset pre-validated by parse_query"),
    }
}

fn hash_file(path: &Path) -> std::io::Result<DigestMap> {
    let bytes = std::fs::read(path)?;
    let blake3 = attestrum_core::hex::encode_32(blake3::hash(&bytes).as_bytes());
    let sha256_bytes: [u8; 32] = Sha256::digest(&bytes).into();
    let sha256 = attestrum_core::hex::encode_32(&sha256_bytes);
    Ok(DigestMap { blake3, sha256 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(bundles: Vec<&str>, manifests: Vec<&str>) -> Args {
        Args {
            model_manifest: PathBuf::from("model.json"),
            binding: PathBuf::from("binding.json"),
            corpus_bundle: bundles.into_iter().map(PathBuf::from).collect(),
            corpus_manifest: manifests.into_iter().map(PathBuf::from).collect(),
            doc: "a".repeat(64),
            certificate_identity: ".*".to_string(),
            certificate_oidc_issuer: ".*".to_string(),
            offline: true,
        }
    }

    #[test]
    fn mismatched_pair_counts_are_args_error() {
        let code = run(args(vec!["a.json", "b.json"], vec!["m.parquet"]));
        assert_eq!(code, ExitCode::ArgsError.as_u8());
    }

    #[test]
    fn empty_corpora_is_args_error() {
        let code = run(args(vec![], vec![]));
        assert_eq!(code, ExitCode::ArgsError.as_u8());
    }

    #[test]
    fn parse_query_hex_blake3() {
        match parse_query(&"b".repeat(64)).unwrap() {
            ProofTarget::Blake3(b) => assert_eq!(b, [0xbb; 32]),
            other => panic!("expected Blake3, got {other:?}"),
        }
    }

    #[test]
    fn parse_query_rejects_uppercase() {
        assert!(parse_query(&"A".repeat(64)).is_err());
    }
}
