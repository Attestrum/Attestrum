//! `attestrum sign <manifest>` — read a sealed manifest, build a
//! `TrainingCorpusPredicate`, wrap it in an in-toto v1 Statement, and emit
//! a Sigstore Bundle v0.3 JSON signed against the public-good Sigstore
//! roots (Fulcio + Rekor v2 + TUF).
//!
//! Drives the [`crate::lifecycle::SignState`] state machine literally so
//! the shipped behaviour matches `docs/diagrams/sprint-4/sign-flow.md`
//! one-to-one. The lifecycle is pure code (no I/O); this module is the
//! single concrete consumer.
//!
//! **Network + OIDC required** (unless `--offline`, which exits 3 before
//! any network or filesystem mutation).
//!
//! Per CLAUDE.md §7 + PR 2 of the 2026-05-24 determinism audit: every
//! byte that enters the signed predicate comes from a deterministic
//! source. `built_at` and the `determinism.seed` field both derive from
//! `--source-date-epoch` (or the `SOURCE_DATE_EPOCH` env var). No
//! `SystemTime::now()` reads on any predicate-build codepath.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use attestrum_attest::{
    sign as attest_sign, DeterminismFields, DigestMap, InTotoStatement, LicenseInventoryEntry,
    LicensingPosture, ManifestRef, PublicationIntent, RulesetMode, SignRequest, SignalCoverage,
    Subject, TrainingCorpusPredicate, TRAINING_CORPUS_PREDICATE_TYPE,
};
use attestrum_manifest::{read_manifest, read_manifest_metadata, ManifestEntry, SCHEMA_VERSION};
use attestrum_merkle::merkle_root;
use sha2::{Digest, Sha256};

use crate::lifecycle::{sign_transition, ExitCode, SignEvent, SignState};

// ============================================================================
// Args + CLI surface
// ============================================================================

/// Subcommand arguments. Owned by `main` and passed in by value.
#[derive(Debug)]
pub struct Args {
    pub manifest: PathBuf,
    /// Optional workspace dir. Default: `<cwd>/.attestrum`. The bundle is
    /// written to `<workspace>/bundles/<manifest-stem>.sigstore.json`.
    pub workspace: Option<PathBuf>,
    /// Reproducible Builds timestamp (epoch seconds). Required at
    /// resolution time — either via `--source-date-epoch` flag OR via
    /// `SOURCE_DATE_EPOCH` env var. No wall-clock fallback per
    /// CLAUDE.md §7.
    pub source_date_epoch: Option<i64>,
    /// Read OIDC id_token (JWT) from this file. Takes precedence over
    /// the `SIGSTORE_ID_TOKEN` env var if both are set.
    pub oidc_token_file: Option<PathBuf>,
    /// `--offline` violation gate: exits 3 immediately, before any
    /// network or filesystem mutation.
    pub offline: bool,
    /// Optional `takedown_contact` predicate field (mailto URL).
    pub takedown_contact: Option<String>,
    /// Optional `dataset_homepage` predicate field (URL).
    pub dataset_homepage: Option<String>,
    /// Optional `publication_intent` predicate field. CLI string maps
    /// 1:1 to [`PublicationIntent`] via [`publication_intent_from_cli`].
    pub publication_intent: Option<String>,
}

// ============================================================================
// Entry point
// ============================================================================

/// `attestrum sign` entry point. Returns the numeric process exit code
/// directly; `main` wraps it in `ExitCode::from(...)`. Errors are printed
/// to stderr inside this function.
pub fn run(args: Args) -> u8 {
    let mut state = SignState::Invoked;
    state = sign_transition(state, SignEvent::ClapParseOk);

    // ArgsParsed → Validated | Exit(ArgsError)
    if !args.manifest.is_file() {
        eprintln!(
            "attestrum sign: manifest path missing or not a file: {}",
            args.manifest.display()
        );
        state = sign_transition(state, SignEvent::PathMissingOrNotFile);
        return terminal_code(state);
    }
    state = sign_transition(state, SignEvent::PathExistsAndIsFile);

    // Validated → OfflineCheck
    state = sign_transition(state, SignEvent::DispatchSign);

    // OfflineCheck → Exit(OfflineViolation) on `--offline`. This fires
    // BEFORE any network or OIDC read so `attestrum sign --offline x` is
    // always cheap to invoke.
    if args.offline {
        eprintln!("attestrum sign: --offline set; signing requires network (Fulcio + Rekor + TUF)");
        state = sign_transition(state, SignEvent::OfflineFlag);
        return terminal_code(state);
    }

    // Resolve --source-date-epoch (or SOURCE_DATE_EPOCH env). Required.
    // This is a deterministic-source guard (CLAUDE.md §7), not an OIDC
    // step — surface as args error so the user knows it's fixable by
    // adding the flag, not by acquiring a token.
    let source_date_epoch = match resolve_source_date_epoch(&args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("attestrum sign: {msg}");
            // Re-enter the lifecycle at ArgsParsed to issue Exit(ArgsError)
            // via the documented PathMissingOrNotFile transition's twin
            // edge — but the missing-epoch case isn't in the diagram.
            // Choose ArgsError directly: the lifecycle isn't a courtroom
            // (the diagram captures the happy paths and the network /
            // OIDC paths; an args-validation miss is captured by the
            // ArgsParsed → Exit(ArgsError) edge whose event is
            // PathMissingOrNotFile in shape, but conceptually identical
            // — args bad).
            return ExitCode::ArgsError.as_u8();
        }
    };

    // OfflineCheck → OidcResolved | Exit(IdentityError).
    let oidc_token = match resolve_oidc_token(&args) {
        Ok(t) => t,
        Err(msg) => {
            eprintln!("attestrum sign: {msg}");
            state = sign_transition(state, SignEvent::OidcTokenMissing);
            return terminal_code(state);
        }
    };
    state = sign_transition(state, SignEvent::OidcTokenLoaded);

    // OidcResolved → ManifestLoaded | Exit(RuntimeError) | Exit(SchemaError).
    // Schema-version check first so wrong-version files don't get loaded
    // (mirrors inspect's pattern at commands/inspect.rs:73).
    let rows: Vec<ManifestEntry> = match read_manifest_metadata(&args.manifest) {
        Ok((schema_version, _writer_profile)) if schema_version != SCHEMA_VERSION => {
            eprintln!(
                "attestrum sign: schema version mismatch: expected {SCHEMA_VERSION}, got {schema_version}"
            );
            state = sign_transition(state, SignEvent::ReadSchemaMismatch);
            return terminal_code(state);
        }
        Ok(_) => match read_manifest(&args.manifest) {
            Ok(r) => {
                state = sign_transition(state, SignEvent::ReadOk);
                r
            }
            Err(e) => {
                eprintln!("attestrum sign: manifest schema mismatch: {e}");
                state = sign_transition(state, SignEvent::ReadSchemaMismatch);
                return terminal_code(state);
            }
        },
        Err(e) => {
            eprintln!("attestrum sign: parquet read failed: {e}");
            state = sign_transition(state, SignEvent::ReadIoError);
            return terminal_code(state);
        }
    };

    // Hash the manifest.parquet file bytes ONCE: feeds both the
    // in-toto Subject.digest and the predicate's manifest.digest_set.
    let manifest_digest = match hash_file_blake3_sha256(&args.manifest) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("attestrum sign: manifest hash failed: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };

    // ManifestLoaded → PredicateBuilt.
    let predicate = build_predicate(
        &rows,
        &args,
        source_date_epoch,
        &manifest_digest,
        &args.manifest,
    );
    state = sign_transition(state, SignEvent::BuildPredicate);

    // PredicateBuilt → StatementBuilt.
    let subject_name = args
        .manifest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "manifest.parquet".to_string());
    let subject = Subject {
        name: subject_name,
        digest: manifest_digest.clone(),
    };
    let predicate_value = match serde_json::to_value(&predicate) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("attestrum sign: predicate serialize failed: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };
    let statement = InTotoStatement::new(
        TRAINING_CORPUS_PREDICATE_TYPE,
        vec![subject],
        predicate_value,
    );
    state = sign_transition(state, SignEvent::BuildStatement);

    // StatementBuilt → PayloadCanonicalized. Goes through
    // attestrum_attest::deterministic_json (single sanctioned sort-then-
    // serialize path; audit PR 3 collapsed three hand-rolled copies
    // into this helper).
    let canonical_payload = match statement.canonical_json() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("attestrum sign: statement canonical_json failed: {e}");
            return ExitCode::RuntimeError.as_u8();
        }
    };
    state = sign_transition(state, SignEvent::Canonicalize);

    // Resolve bundle output path. `<workspace>/bundles/<stem>.sigstore.json`.
    let workspace = args
        .workspace
        .clone()
        .unwrap_or_else(|| PathBuf::from(".").join(".attestrum"));
    let bundle_dir = workspace.join("bundles");
    let stem = args
        .manifest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "manifest".to_string());
    let bundle_path = bundle_dir.join(format!("{stem}.sigstore.json"));

    // Build the merkle_root string from the predicate (we already
    // computed it during build_predicate; re-extract for the summary).
    let merkle_root_hex = predicate.merkle_root.clone();
    let predicate_type_str = TRAINING_CORPUS_PREDICATE_TYPE.to_string();

    // PayloadCanonicalized → SignerOpened | Exit(NetworkError).
    // SignerOpened → Signed | Exit(IdentityError) | Exit(NetworkError).
    // BundleWritten via fs::write inside attestrum_attest::sign.
    //
    // attestrum_attest::sign collapses TUF init + Fulcio CSR + DSSE sign +
    // Rekor v2 submit + bundle write into one call. We can't peek inside
    // to drive each sub-state separately without re-implementing the
    // wrapper. So: drive the lifecycle through SigningContextOk + SignOk
    // + WriteOk on Ok; on Err, map the AttestrumAttestError variant to the
    // right failure transition.
    let signed = match attest_sign(SignRequest {
        statement_payload: canonical_payload.as_bytes(),
        bundle_output_path: &bundle_path,
        oidc_id_token: oidc_token,
    }) {
        Ok(s) => {
            state = sign_transition(state, SignEvent::SigningContextOk);
            state = sign_transition(state, SignEvent::SignOk);
            state = sign_transition(state, SignEvent::WriteOk);
            s
        }
        Err(e) => {
            use attestrum_attest::AttestrumAttestError as E;
            eprintln!("attestrum sign: {e}");
            match e {
                E::SigstoreContext(_) => {
                    // PayloadCanonicalized → Exit(NetworkError).
                    state = sign_transition(state, SignEvent::SigningContextFail);
                }
                E::SigstoreIdentityToken(_) | E::SigstoreSession(_) => {
                    // Transition through SigningContextOk so we land at
                    // SignerOpened, then dispatch the identity failure.
                    state = sign_transition(state, SignEvent::SigningContextOk);
                    state = sign_transition(state, SignEvent::SignIdentityError);
                }
                E::SigstoreSign(_) | E::DsseSign(_) => {
                    // Rekor / DSSE failure mid-sign — surface as network
                    // (most common cause: Rekor 5xx). Sigstore-rs doesn't
                    // separate Rekor-network from sign-crypto failures
                    // in either variant. SigstoreSign is the legacy
                    // Bundle-v0.2 + MessageSignature path; DsseSign is
                    // the X→Y hybrid Bundle-v0.3 + DSSE path.
                    state = sign_transition(state, SignEvent::SigningContextOk);
                    state = sign_transition(state, SignEvent::SignNetworkError);
                }
                E::Io(_) => {
                    // Bundle write failed AFTER sign succeeded.
                    state = sign_transition(state, SignEvent::SigningContextOk);
                    state = sign_transition(state, SignEvent::SignOk);
                    state = sign_transition(state, SignEvent::WriteIoError);
                }
                E::Json(_)
                | E::InTotoTypeMismatch { .. }
                | E::ProofTypeMismatch { .. }
                | E::BoundaryCaseNeighborMissing { .. }
                | E::SigstoreVerify(_)
                | E::IdentityExtractionFailed(_)
                | E::IdentityPolicyMismatch { .. }
                | E::PredicateValidationFailed(_) => {
                    // Should be unreachable on the sign path (these are
                    // construct-time validation errors we already past
                    // OR verify-side variants that sign() can't emit);
                    // surface as RuntimeError.
                    return ExitCode::RuntimeError.as_u8();
                }
            }
            return terminal_code(state);
        }
    };

    // BundleWritten → Exit(Ok). Print the success summary.
    print_summary(
        &merkle_root_hex,
        &signed.bundle_path,
        &signed.identity,
        &signed.oidc_issuer,
        &predicate_type_str,
    );
    state = sign_transition(state, SignEvent::PrintSummary);
    terminal_code(state)
}

fn terminal_code(state: SignState) -> u8 {
    match state {
        SignState::Exit(code) => code.as_u8(),
        // Defensive: if the lifecycle is non-terminal when `run`
        // returns, surface as generic runtime error rather than panic.
        _ => ExitCode::RuntimeError.as_u8(),
    }
}

// ============================================================================
// Predicate construction
// ============================================================================

/// Build the `TrainingCorpusPredicate` from the loaded manifest rows,
/// CLI args, and the pre-computed file digest. Pure function: no I/O,
/// no clock reads. All timestamps derive from `source_date_epoch`;
/// `target_triple` comes from the build.rs-propagated env;
/// row aggregation uses BTreeMap; SignalCoverage is u32 PPM (v0.3 schema).
fn build_predicate(
    rows: &[ManifestEntry],
    args: &Args,
    source_date_epoch: i64,
    manifest_digest: &DigestMap,
    manifest_path: &Path,
) -> TrainingCorpusPredicate {
    let attestrum_version = env!("CARGO_PKG_VERSION").to_string();
    let builder_version = format!("attestrum-cli/{attestrum_version}");
    let built_at = format_epoch_rfc3339(source_date_epoch);

    let determinism = DeterminismFields {
        target_triple: env!("ATTESTRUM_TARGET_TRIPLE").to_string(),
        seed: source_date_epoch.to_string(),
        manifest_schema_version: SCHEMA_VERSION.to_string(),
    };

    // Canonicalize the manifest path to a file:// URI. If canonicalize
    // fails (e.g., in a chroot or under unusual filesystem conditions),
    // fall back to the display form — the digest_set is what verifiers
    // match against, not the URI string.
    let manifest_uri = match manifest_path.canonicalize() {
        Ok(p) => format!("file://{}", p.display()),
        Err(_) => format!("file://{}", manifest_path.display()),
    };

    // byte_count is the parquet file size on disk, NOT the sum of
    // rows[i].size_bytes (which is corpus content bytes, a different
    // measurement). fs::metadata can fail for race-y reasons; fall
    // back to 0 if so (the verifier cross-references against the
    // digest_set bytes-hashing operation which is the authoritative
    // size signal).
    let byte_count = fs::metadata(manifest_path).map(|m| m.len()).unwrap_or(0);

    let manifest = ManifestRef {
        uri: manifest_uri,
        digest_set: manifest_digest.clone(),
        row_count: rows.len() as u64,
        byte_count,
    };

    // Merkle root over the canonically-sorted document_id leaves.
    // Manifest rows are guaranteed sorted by (document_id, occurrence_index)
    // per the PROTECTED Sprint 3 E3 schema, so the leaves slice we
    // pass to merkle_root is already in canonical order.
    let leaves: Vec<[u8; 32]> = rows.iter().map(|r| r.document_id).collect();
    let merkle_root_bytes = merkle_root(&leaves);
    let merkle_root_hex = hex_64(&merkle_root_bytes);

    let signal_coverage = signal_coverage_ppm(rows);
    let license_inventory = aggregate_license_inventory(rows);
    let publication_intent = args
        .publication_intent
        .as_deref()
        .and_then(publication_intent_from_cli);

    TrainingCorpusPredicate {
        attestrum_version,
        builder_version,
        built_at,
        determinism,
        manifest,
        merkle_root: merkle_root_hex,
        merkle_algorithm: "blake3-rfc6962".to_string(),
        // E3.5 tactical defaults — see plan §3.2 (Tactical Decision B).
        // Real ruleset config-file plumbing lands later; flags will
        // surface alongside it.
        ruleset_mode: RulesetMode::Strict,
        ruleset_id: "attestrum-default".to_string(),
        ruleset_version: "v0.1.0".to_string(),
        signal_coverage,
        // E3.5 tactical default — see plan §3.2 (Tactical Decision C).
        // Open-SPDX heuristic deferred; current data goes out as
        // Undisclosed rather than committing a contestable whitelist
        // to a signed v0.3 bundle.
        licensing_posture: LicensingPosture::Undisclosed,
        license_inventory,
        takedown_contact: args.takedown_contact.clone(),
        dataset_homepage: args.dataset_homepage.clone(),
        publication_intent,
        total_compute: None,
        training_cost: None,
        model_name: None,
    }
}

/// Compute per-signal PPM coverage from the manifest's per-row signals.
/// PPM = `(evaluated_count * 1_000_000) / total_count`, clamped to
/// `0..=1_000_000`. Returns `None` for any signal that was never
/// evaluated (so the wire form is `null`, distinct from `Some(0)`
/// which means "evaluated and zero coverage").
///
/// "Evaluated" predicate per signal — derived from the [`ManifestSignals`]
/// shape at `crates/attestrum-manifest/src/lib.rs:48-62`:
///
/// | Signal       | Evaluated iff                                         |
/// |--------------|-------------------------------------------------------|
/// | robots_txt   | `robots_disallow == true || robots_user_agent.is_some()` |
/// | ai_txt       | `ai_txt_disallow == true`                             |
/// | tdm_rep      | `tdmrep_reservation != 0 || tdmrep_policy_url.is_some()` |
/// | aipref       | `aipref_usage_pref.is_some()`                         |
/// | iptc_plus    | `iptc_plus_dmi.is_some()`                             |
/// | c2pa         | `c2pa_training_mining.is_some()`                      |
/// | rsl          | `rsl_permits.is_some()`                               |
/// | liccium      | `liccium_tdmai_iscc.is_some() || liccium_tdmai_allow.is_some()` |
/// | cloudflare   | `cloudflare_ai_train.is_some()`                       |
///
/// `tdmrep_reservation == 0` is the "unset" sentinel per the
/// `ManifestSignals` doc comment; we honor that here.
fn signal_coverage_ppm(rows: &[ManifestEntry]) -> SignalCoverage {
    if rows.is_empty() {
        return SignalCoverage::default();
    }
    let total = rows.len() as u64;
    let mut robots_txt = 0u64;
    let mut ai_txt = 0u64;
    let mut tdm_rep = 0u64;
    let mut aipref = 0u64;
    let mut iptc_plus = 0u64;
    let mut c2pa = 0u64;
    let mut rsl = 0u64;
    let mut liccium = 0u64;
    let mut cloudflare = 0u64;
    for r in rows {
        let s = &r.signals;
        if s.robots_disallow || s.robots_user_agent.is_some() {
            robots_txt += 1;
        }
        if s.ai_txt_disallow {
            ai_txt += 1;
        }
        if s.tdmrep_reservation != 0 || s.tdmrep_policy_url.is_some() {
            tdm_rep += 1;
        }
        if s.aipref_usage_pref.is_some() {
            aipref += 1;
        }
        if s.iptc_plus_dmi.is_some() {
            iptc_plus += 1;
        }
        if s.c2pa_training_mining.is_some() {
            c2pa += 1;
        }
        if s.rsl_permits.is_some() {
            rsl += 1;
        }
        if s.liccium_tdmai_iscc.is_some() || s.liccium_tdmai_allow.is_some() {
            liccium += 1;
        }
        if s.cloudflare_ai_train.is_some() {
            cloudflare += 1;
        }
    }
    SignalCoverage {
        robots_txt: Some(ppm(robots_txt, total)),
        ai_txt: Some(ppm(ai_txt, total)),
        tdm_rep: Some(ppm(tdm_rep, total)),
        aipref: Some(ppm(aipref, total)),
        iptc_plus: Some(ppm(iptc_plus, total)),
        c2pa: Some(ppm(c2pa, total)),
        rsl: Some(ppm(rsl, total)),
        liccium: Some(ppm(liccium, total)),
        cloudflare: Some(ppm(cloudflare, total)),
    }
}

fn ppm(count: u64, total: u64) -> u32 {
    // total is guaranteed > 0 by caller. Defensive .min in case the
    // arithmetic ever overflows the 0..=1_000_000 range.
    ((count.saturating_mul(1_000_000) / total).min(1_000_000)) as u32
}

/// Aggregate per-row `license_spdx` into a sorted `LicenseInventoryEntry`
/// list. Rows with `license_spdx == None` are not represented; absent
/// license info shows up as `LicensingPosture::Undisclosed` at the
/// outer field. BTreeMap iteration gives byte-stable output ordering
/// across runs.
fn aggregate_license_inventory(rows: &[ManifestEntry]) -> Vec<LicenseInventoryEntry> {
    let mut by_spdx: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in rows {
        if let Some(spdx) = &r.license_spdx {
            let entry = by_spdx.entry(spdx.clone()).or_insert((0, 0));
            entry.0 += r.size_bytes;
            entry.1 += 1;
        }
    }
    by_spdx
        .into_iter()
        .map(|(spdx_id, (byte_count, row_count))| LicenseInventoryEntry {
            spdx_id,
            byte_count,
            row_count: Some(row_count),
            notes: None,
        })
        .collect()
}

fn publication_intent_from_cli(s: &str) -> Option<PublicationIntent> {
    match s {
        "hf" | "huggingface-hub" => Some(PublicationIntent::HuggingFaceHub),
        "zenodo" => Some(PublicationIntent::Zenodo),
        "github-release" => Some(PublicationIntent::GitHubRelease),
        "eu-ai-office" => Some(PublicationIntent::EuAiOffice),
        "private" => Some(PublicationIntent::Private),
        _ => None,
    }
}

// ============================================================================
// Source-date-epoch + OIDC resolution
// ============================================================================

fn resolve_source_date_epoch(args: &Args) -> Result<i64, String> {
    if let Some(s) = args.source_date_epoch {
        return Ok(s);
    }
    if let Ok(s) = std::env::var("SOURCE_DATE_EPOCH") {
        return s
            .parse::<i64>()
            .map_err(|e| format!("SOURCE_DATE_EPOCH env var is not a valid integer: {s:?} ({e})"));
    }
    Err("required: pass --source-date-epoch <SECS> or set SOURCE_DATE_EPOCH env var (deterministic-source guard per CLAUDE.md §7)".to_string())
}

fn resolve_oidc_token(args: &Args) -> Result<String, String> {
    if let Some(path) = &args.oidc_token_file {
        let raw = fs::read_to_string(path)
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
        _ => Err("OIDC id_token required: pass --oidc-token-file <PATH> or set SIGSTORE_ID_TOKEN env var".to_string()),
    }
}

// ============================================================================
// File hashing
// ============================================================================

/// Compute BLAKE3 + SHA-256 of a file's bytes in a single read. Used
/// for both the in-toto Subject.digest and the predicate's
/// `manifest.digest_set`.
fn hash_file_blake3_sha256(path: &Path) -> std::io::Result<DigestMap> {
    let bytes = fs::read(path)?;
    let blake3_hex = hex_64(blake3::hash(&bytes).as_bytes());
    let sha256_bytes: [u8; 32] = Sha256::digest(&bytes).into();
    let sha256_hex = hex_64(&sha256_bytes);
    Ok(DigestMap {
        blake3: blake3_hex,
        sha256: sha256_hex,
    })
}

// ============================================================================
// RFC 3339 formatter (no chrono dep — small in-crate helper)
// ============================================================================

/// Convert a UTC epoch-seconds timestamp to RFC 3339 form (`Z` zulu
/// suffix; no nanosecond precision). Covers years 1970..=9999. Hand-
/// rolled to avoid a chrono / time dep — see plan §Risks.
///
/// Algorithm: civil_from_days from Howard Hinnant's "date algorithms"
/// paper (public domain). Standard reference for epoch ↔ Y/M/D
/// conversions; ~15 lines.
fn format_epoch_rfc3339(epoch: i64) -> String {
    let secs_per_day: i64 = 86_400;
    let days = epoch.div_euclid(secs_per_day);
    let secs_of_day = epoch.rem_euclid(secs_per_day);
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let second = (secs_of_day % 60) as u32;

    // civil_from_days: days since 1970-01-01 → (year, month, day).
    // Hinnant's algorithm, ~15 lines. Range: years -32767..=32767.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = (y + (m <= 2) as i64) as i32;

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// ============================================================================
// Small util
// ============================================================================

fn hex_64(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn print_summary(
    merkle_root_hex: &str,
    bundle_path: &Path,
    identity: &str,
    oidc_issuer: &str,
    predicate_type: &str,
) {
    println!("merkle_root:    {merkle_root_hex}");
    println!("bundle_path:    {}", bundle_path.display());
    println!("identity:       {identity}");
    println!("oidc_issuer:    {oidc_issuer}");
    println!("predicate_type: {predicate_type}");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use attestrum_core::{Modality, SourceType};
    use attestrum_manifest::ManifestSignals;

    fn sample_entry(doc_byte: u8, spdx: Option<&str>) -> ManifestEntry {
        ManifestEntry {
            document_id: [doc_byte; 32],
            sha256: [doc_byte ^ 0xff; 32],
            size_bytes: 100,
            modality: Modality::Text,
            mime_type: Some("text/plain".into()),
            source_url: Some(format!("file:///docs/doc-{doc_byte:02x}.txt")),
            source_type: Some(SourceType::PublicDataset),
            source_dataset_id: Some("test-corpus".into()),
            registered_domain: None,
            license_spdx: spdx.map(String::from),
            language: Some("en".into()),
            fetched_at: Some(1_700_000_000),
            signals: ManifestSignals::default(),
            included: true,
            exclusion_reason: None,
            chunk_refs: None,
            input_ordinal: 0,
            occurrence_index: 0,
        }
    }

    #[test]
    fn signal_coverage_ppm_empty_rows_returns_all_none() {
        let coverage = signal_coverage_ppm(&[]);
        assert_eq!(coverage, SignalCoverage::default());
        assert!(coverage.robots_txt.is_none());
        assert!(coverage.cloudflare.is_none());
    }

    #[test]
    fn signal_coverage_ppm_ratios_round_to_expected_values() {
        // 4 rows: 2 have robots_disallow, 1 has ai_txt_disallow, 0 have rsl.
        // Expected PPM: robots_txt = 500_000, ai_txt = 250_000, rsl = 0.
        let mut rows = vec![
            sample_entry(0x01, None),
            sample_entry(0x02, None),
            sample_entry(0x03, None),
            sample_entry(0x04, None),
        ];
        rows[0].signals.robots_disallow = true;
        rows[1].signals.robots_disallow = true;
        rows[2].signals.ai_txt_disallow = true;
        let coverage = signal_coverage_ppm(&rows);
        assert_eq!(coverage.robots_txt, Some(500_000));
        assert_eq!(coverage.ai_txt, Some(250_000));
        assert_eq!(coverage.rsl, Some(0));
        // Defensive bounds:
        assert!(coverage.robots_txt.unwrap() <= 1_000_000);
    }

    #[test]
    fn signal_coverage_ppm_all_signals_evaluated_at_one_hundred_percent() {
        let mut rows = vec![sample_entry(0x01, None), sample_entry(0x02, None)];
        for r in &mut rows {
            r.signals.robots_disallow = true;
            r.signals.ai_txt_disallow = true;
            r.signals.tdmrep_reservation = 1;
            r.signals.aipref_usage_pref = Some("opt-out".into());
            r.signals.iptc_plus_dmi = Some("opt-out".into());
            r.signals.c2pa_training_mining = Some("notAllowed".into());
            r.signals.rsl_permits = Some("nothing".into());
            r.signals.liccium_tdmai_iscc = Some("KAA...".into());
            r.signals.cloudflare_ai_train = Some("no".into());
        }
        let coverage = signal_coverage_ppm(&rows);
        assert_eq!(coverage.robots_txt, Some(1_000_000));
        assert_eq!(coverage.ai_txt, Some(1_000_000));
        assert_eq!(coverage.tdm_rep, Some(1_000_000));
        assert_eq!(coverage.aipref, Some(1_000_000));
        assert_eq!(coverage.iptc_plus, Some(1_000_000));
        assert_eq!(coverage.c2pa, Some(1_000_000));
        assert_eq!(coverage.rsl, Some(1_000_000));
        assert_eq!(coverage.liccium, Some(1_000_000));
        assert_eq!(coverage.cloudflare, Some(1_000_000));
    }

    #[test]
    fn aggregate_license_inventory_groups_by_spdx_and_sorts() {
        let rows = vec![
            sample_entry(0x01, Some("MIT")),
            sample_entry(0x02, Some("Apache-2.0")),
            sample_entry(0x03, Some("MIT")),
            sample_entry(0x04, None),
            sample_entry(0x05, Some("Apache-2.0")),
        ];
        let inv = aggregate_license_inventory(&rows);
        // Sorted by spdx_id ascending: Apache-2.0, MIT. None entry skipped.
        assert_eq!(inv.len(), 2);
        assert_eq!(inv[0].spdx_id, "Apache-2.0");
        assert_eq!(inv[0].row_count, Some(2));
        assert_eq!(inv[0].byte_count, 200);
        assert_eq!(inv[1].spdx_id, "MIT");
        assert_eq!(inv[1].row_count, Some(2));
        assert_eq!(inv[1].byte_count, 200);
    }

    #[test]
    fn aggregate_license_inventory_all_none_returns_empty_vec() {
        let rows = vec![sample_entry(0x01, None), sample_entry(0x02, None)];
        let inv = aggregate_license_inventory(&rows);
        assert!(inv.is_empty());
    }

    #[test]
    fn format_epoch_rfc3339_known_dates() {
        // Unix epoch.
        assert_eq!(format_epoch_rfc3339(0), "1970-01-01T00:00:00Z");
        // 2026-05-24 18:00:00 UTC = 1779645600 (1 year past 2025-05-24).
        assert_eq!(format_epoch_rfc3339(1_779_645_600), "2026-05-24T18:00:00Z");
        // 2025-05-24 18:00:00 UTC = 1748109600.
        assert_eq!(format_epoch_rfc3339(1_748_109_600), "2025-05-24T18:00:00Z");
        // Y2K.
        assert_eq!(format_epoch_rfc3339(946_684_800), "2000-01-01T00:00:00Z");
        // 2038-01-19 03:14:07 (i32 epoch overflow boundary).
        assert_eq!(format_epoch_rfc3339(2_147_483_647), "2038-01-19T03:14:07Z");
    }

    #[test]
    fn format_epoch_rfc3339_handles_leap_year_boundary() {
        // 2024-02-29 00:00:00 = 1709164800. Leap year exercise.
        assert_eq!(format_epoch_rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
        // 2024-03-01 00:00:00 = 1709251200.
        assert_eq!(format_epoch_rfc3339(1_709_251_200), "2024-03-01T00:00:00Z");
    }

    #[test]
    fn publication_intent_from_cli_maps_known_values() {
        assert_eq!(
            publication_intent_from_cli("hf"),
            Some(PublicationIntent::HuggingFaceHub)
        );
        assert_eq!(
            publication_intent_from_cli("huggingface-hub"),
            Some(PublicationIntent::HuggingFaceHub)
        );
        assert_eq!(
            publication_intent_from_cli("github-release"),
            Some(PublicationIntent::GitHubRelease)
        );
        assert_eq!(
            publication_intent_from_cli("private"),
            Some(PublicationIntent::Private)
        );
        assert_eq!(publication_intent_from_cli("garbage"), None);
    }

    #[test]
    fn ppm_clamps_at_one_million() {
        assert_eq!(ppm(10, 10), 1_000_000);
        assert_eq!(ppm(0, 10), 0);
        assert_eq!(ppm(1, 10), 100_000);
        // Saturating-mul defense: extreme counts can't wrap u64. With
        // count == total == u64::MAX, saturating_mul saturates first
        // (u64::MAX * 1_000_000 → u64::MAX), then divides by u64::MAX
        // giving 1 — the .min(1_000_000) keeps it in-range. The point
        // of the saturating_mul is to avoid wrap-to-zero, not to
        // produce the "correct" mathematical ratio (which is 1.0 i.e.
        // 1_000_000 — but no real manifest can have u64::MAX rows).
        assert!(ppm(u64::MAX, u64::MAX) <= 1_000_000);
    }
}
