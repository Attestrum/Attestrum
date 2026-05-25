//! `attestrum verify <bundle>` — read a Sigstore Bundle v0.3, cryptographically
//! verify against the public-good Sigstore trust root, match identity
//! against operator-supplied regex policy, light-weight-validate the
//! in-toto predicate against the v0.3 training-corpus schema, print a
//! green/red verdict.
//!
//! Drives the [`crate::lifecycle::VerifyState`] state machine literally so
//! the shipped behaviour matches `docs/diagrams/sprint-4/verify-flow.md`
//! one-to-one. The lifecycle is pure code (no I/O); this module is the
//! single concrete consumer.
//!
//! **Network unless `--offline`** — sigstore-rs's TUF refresh against the
//! public-good trusted root, cached at `~/.sigstore`. Rekor online
//! re-check is skipped under `--offline`; the bundle's embedded signed
//! inclusion proof + RFC3161 TSA timestamp are still validated against
//! the cached trust root.
//!
//! **E4 deferrals** (documented in `verify-flow.md` body):
//! - `<workspace>/trust/` cache layout → follow-up; E4 uses sigstore-rs
//!   default `~/.sigstore`.
//! - `--manifest`-omitted auto-resolution from `bundle.subject[0].name` →
//!   follow-up; E4 requires explicit `--manifest`.
//! - `<workspace>/attestrum.toml`'s `[verify]` block for default identity-
//!   policy → follow-up; E4 requires both `--certificate-identity` +
//!   `--certificate-oidc-issuer` flags.

use std::path::PathBuf;

use attestrum_attest::{
    verify as attest_verify, AttestrumAttestError, VerifiedAttestation, VerifyRequest,
};

use crate::lifecycle::{verify_transition, ExitCode, VerifyEvent, VerifyState};

// ============================================================================
// Args + CLI surface
// ============================================================================

#[derive(Debug)]
pub struct Args {
    pub bundle: PathBuf,
    pub manifest: PathBuf,
    pub certificate_identity: String,
    pub certificate_oidc_issuer: String,
    pub offline: bool,
    pub print_predicate: bool,
}

// ============================================================================
// Entry point
// ============================================================================

pub fn run(args: Args) -> u8 {
    let mut state = VerifyState::Invoked;
    state = verify_transition(state, VerifyEvent::ClapParseOk);

    // ArgsParsed → Validated | Exit(ArgsError). Both bundle + manifest
    // must exist as files.
    if !args.bundle.is_file() {
        eprintln!(
            "attestrum verify: bundle path missing or not a file: {}",
            args.bundle.display()
        );
        state = verify_transition(state, VerifyEvent::PathMissingOrNotFile);
        return terminal_code(state);
    }
    if !args.manifest.is_file() {
        eprintln!(
            "attestrum verify: manifest path missing or not a file: {}",
            args.manifest.display()
        );
        state = verify_transition(state, VerifyEvent::PathMissingOrNotFile);
        return terminal_code(state);
    }
    state = verify_transition(state, VerifyEvent::PathsExistAndAreFiles);

    // Hand off to attestrum_attest::verify, which collapses bundle read +
    // identity extract + trust root refresh + crypto verify + identity
    // regex check + payload decode + predicate deserialize into one
    // call. We can't peek inside to drive each sub-state separately
    // without re-implementing the wrapper; instead, on Ok we advance
    // the lifecycle through the documented happy path, and on Err we
    // map the AttestrumAttestError variant to the right failure transition.
    let verified: VerifiedAttestation = match attest_verify(VerifyRequest {
        bundle_path: &args.bundle,
        manifest_path: &args.manifest,
        identity_regex: &args.certificate_identity,
        issuer_regex: &args.certificate_oidc_issuer,
        offline: args.offline,
    }) {
        Ok(v) => {
            state = verify_transition(state, VerifyEvent::BundleReadOk);
            state = verify_transition(state, VerifyEvent::IdentityExtractOk);
            state = verify_transition(state, VerifyEvent::TrustRootOk);
            state = verify_transition(state, VerifyEvent::SigstoreVerifyOk);
            state = verify_transition(state, VerifyEvent::IdentityRegexMatchOk);
            state = verify_transition(state, VerifyEvent::PayloadDecodeOk);
            state = verify_transition(state, VerifyEvent::PredicateDeserializeOk);
            v
        }
        Err(e) => {
            eprintln!("attestrum verify: {e}");
            match e {
                AttestrumAttestError::Io(_) => {
                    // Bundle read I/O (the manifest read I/O would be
                    // surfaced inside sigstore-rs's verify call, classed
                    // as a SigstoreVerify error). Treat as runtime.
                    state = verify_transition(state, VerifyEvent::BundleReadIoError);
                }
                AttestrumAttestError::Json(_) => {
                    // Bundle JSON parse failure — malformed bundle file.
                    // No clean transition since the diagram doesn't model
                    // "bundle JSON parse fail" as distinct from
                    // "bundle read I/O fail"; surface as RuntimeError via
                    // the closest documented edge (BundleReadIoError).
                    state = verify_transition(state, VerifyEvent::BundleReadIoError);
                }
                AttestrumAttestError::IdentityExtractionFailed(_) => {
                    // Bundle was readable + JSON-parseable but the cert
                    // wasn't extractable. The lifecycle has BundleLoaded
                    // before IdentityExtracted — advance to BundleLoaded
                    // first then surface IdentityExtractFailed.
                    state = verify_transition(state, VerifyEvent::BundleReadOk);
                    state = verify_transition(state, VerifyEvent::IdentityExtractFailed);
                }
                AttestrumAttestError::IdentityPolicyMismatch { .. } => {
                    // Crypto verified successfully but identity regex
                    // didn't match the extracted values.
                    state = verify_transition(state, VerifyEvent::BundleReadOk);
                    state = verify_transition(state, VerifyEvent::IdentityExtractOk);
                    state = verify_transition(state, VerifyEvent::TrustRootOk);
                    state = verify_transition(state, VerifyEvent::SigstoreVerifyOk);
                    state = verify_transition(state, VerifyEvent::IdentityRegexMismatch);
                }
                AttestrumAttestError::SigstoreContext(_) => {
                    // TUF refresh failed. Map to OfflineWithStaleCache if
                    // --offline was set (stale cache + no refresh allowed),
                    // else TufRefreshFail (network).
                    state = verify_transition(state, VerifyEvent::BundleReadOk);
                    state = verify_transition(state, VerifyEvent::IdentityExtractOk);
                    state = verify_transition(
                        state,
                        if args.offline {
                            VerifyEvent::OfflineWithStaleCache
                        } else {
                            VerifyEvent::TufRefreshFail
                        },
                    );
                }
                AttestrumAttestError::SigstoreVerify(_) => {
                    // Cryptographic verify failed (cert chain, sig, Rekor,
                    // TSA).
                    state = verify_transition(state, VerifyEvent::BundleReadOk);
                    state = verify_transition(state, VerifyEvent::IdentityExtractOk);
                    state = verify_transition(state, VerifyEvent::TrustRootOk);
                    state = verify_transition(state, VerifyEvent::SigstoreVerifyFail);
                }
                AttestrumAttestError::PredicateValidationFailed(_) => {
                    // Crypto verified + identity matched + payload decoded
                    // OK; only the predicate deserialise failed.
                    state = verify_transition(state, VerifyEvent::BundleReadOk);
                    state = verify_transition(state, VerifyEvent::IdentityExtractOk);
                    state = verify_transition(state, VerifyEvent::TrustRootOk);
                    state = verify_transition(state, VerifyEvent::SigstoreVerifyOk);
                    state = verify_transition(state, VerifyEvent::IdentityRegexMatchOk);
                    state = verify_transition(state, VerifyEvent::PayloadDecodeOk);
                    state = verify_transition(state, VerifyEvent::PredicateDeserializeFail);
                }
                // Other variants (SigstoreSign*, InTotoTypeMismatch, etc.)
                // shouldn't surface from the verify codepath; treat as
                // runtime defensively.
                _ => {
                    return ExitCode::RuntimeError.as_u8();
                }
            }
            return terminal_code(state);
        }
    };

    // SchemaValidated → Exit(Ok). Print success summary + (optionally)
    // the canonical-JSON predicate body.
    print_summary(&verified);
    if args.print_predicate {
        match attestrum_attest::deterministic_json(&verified.predicate) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("attestrum verify: --print-predicate serialise failed: {e}");
                return ExitCode::RuntimeError.as_u8();
            }
        }
    }
    state = verify_transition(state, VerifyEvent::PrintSummary);
    terminal_code(state)
}

fn terminal_code(state: VerifyState) -> u8 {
    match state {
        VerifyState::Exit(code) => code.as_u8(),
        _ => ExitCode::RuntimeError.as_u8(),
    }
}

fn print_summary(v: &VerifiedAttestation) {
    println!("verified:        GREEN");
    println!("identity:        {}", v.identity);
    println!("oidc_issuer:     {}", v.oidc_issuer);
    println!("predicate_type:  {}", v.predicate_type);
    println!("merkle_root:     {}", v.predicate.merkle_root);
    println!("integrated_time: {}", v.integrated_time);
    println!("log_index:       {}", v.log_index);
    println!("bundle_path:     {}", v.bundle_path.display());
}
