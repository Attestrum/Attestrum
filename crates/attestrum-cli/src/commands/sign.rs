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
use attestrum_cas::stream_hash_path;
use attestrum_manifest::{
    read_manifest_metadata, ManifestBatchReader, ManifestEntry, SCHEMA_VERSION,
};
use attestrum_merkle::merkle_root;

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
    let oidc_token =
        match crate::commands::oidc::resolve_oidc_token(args.oidc_token_file.as_deref(), false) {
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
    // read_manifest_metadata reads only the Parquet footer (no row decode),
    // so its failure is the ReadIoError / parse-init path.
    match read_manifest_metadata(&args.manifest) {
        Ok((schema_version, _writer_profile)) if schema_version != SCHEMA_VERSION => {
            eprintln!(
                "attestrum sign: schema version mismatch: expected {SCHEMA_VERSION}, got {schema_version}"
            );
            state = sign_transition(state, SignEvent::ReadSchemaMismatch);
            return terminal_code(state);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("attestrum sign: parquet read failed: {e}");
            state = sign_transition(state, SignEvent::ReadIoError);
            return terminal_code(state);
        }
    }

    // Stream the manifest in CONSTANT memory, accumulating only what the
    // predicate needs (the document_id leaf vector + per-signal counters +
    // the license BTreeMap) — never the whole Vec<ManifestEntry>, which is
    // ~30 GB at 100M rows and OOMs a 16 GB runner. Mirrors the streaming
    // `attestrum merge` and `attestrum compose` (aggregate_manifest) paths.
    let aggregates = match stream_predicate_aggregates(&args.manifest) {
        Ok(a) => {
            state = sign_transition(state, SignEvent::ReadOk);
            a
        }
        Err(e) => {
            eprintln!("attestrum sign: manifest schema mismatch: {e}");
            state = sign_transition(state, SignEvent::ReadSchemaMismatch);
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
        &aggregates,
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
    aggregates: &PredicateAggregates,
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
        row_count: aggregates.row_count,
        byte_count,
    };

    // Merkle root over the canonically-sorted document_id leaves. Manifest
    // rows are guaranteed sorted by (document_id, occurrence_index) per the
    // PROTECTED Sprint 3 E3 schema, and the streaming aggregator collected
    // the document_id leaves in that same on-disk order — so this root is
    // byte-identical to the prior full-slice computation.
    let merkle_root_hex = hex_64(&aggregates.merkle_root);

    let signal_coverage = aggregates.signal_coverage.clone();
    let license_inventory = aggregates.license_inventory.clone();
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

// ============================================================================
// Streaming predicate aggregation — constant memory, scales to 100M+ rows
// ============================================================================

/// The predicate-relevant aggregates of a manifest, computed in a single
/// constant-memory pass. Replaces the prior "load the whole
/// `Vec<ManifestEntry>`" approach, which is ~30 GB at 100M rows and OOMs a
/// 16 GB runner — `sign` was the last non-streaming step in the seal pipeline
/// (`build` and `merge` already stream).
struct PredicateAggregates {
    /// RFC 6962 BLAKE3 Merkle root over the document_id leaves, in on-disk
    /// (canonical) order — byte-identical to the prior full-slice root.
    merkle_root: [u8; 32],
    /// Total rows == leaf count == `manifest.row_count`.
    row_count: u64,
    signal_coverage: SignalCoverage,
    license_inventory: Vec<LicenseInventoryEntry>,
}

/// Constant-memory accumulator that folds one [`ManifestEntry`] at a time,
/// holding only the document_id leaf vector (~32 B/row — the same vector
/// `attestrum merge` and `attestrum compose` hold), the nine per-signal
/// "evaluated" counters, and the per-SPDX license map. Mirrors
/// `attestrum_compose`'s `Aggregator`. The `add` per-row logic and the
/// `finish` finalization are byte-for-byte equivalent to the prior
/// `signal_coverage_ppm` + `aggregate_license_inventory` slice functions,
/// which are retained as test oracles (see `mod tests`).
#[derive(Default)]
struct PredicateAggregator {
    leaves: Vec<[u8; 32]>,
    row_count: u64,
    // Per-signal "evaluated" counts — see the evaluated-iff table on the
    // `signal_coverage_ppm` test oracle this reproduces.
    robots_txt: u64,
    ai_txt: u64,
    tdm_rep: u64,
    aipref: u64,
    iptc_plus: u64,
    c2pa: u64,
    rsl: u64,
    liccium: u64,
    cloudflare: u64,
    // spdx_id -> (byte_count, row_count). BTreeMap gives byte-stable output
    // ordering across runs (the determinism the license inventory needs).
    license_by_spdx: BTreeMap<String, (u64, u64)>,
}

impl PredicateAggregator {
    fn add(&mut self, e: &ManifestEntry) {
        self.leaves.push(e.document_id);
        self.row_count += 1;

        let s = &e.signals;
        if s.robots_disallow || s.robots_user_agent.is_some() {
            self.robots_txt += 1;
        }
        if s.ai_txt_disallow {
            self.ai_txt += 1;
        }
        if s.tdmrep_reservation != 0 || s.tdmrep_policy_url.is_some() {
            self.tdm_rep += 1;
        }
        if s.aipref_usage_pref.is_some() {
            self.aipref += 1;
        }
        if s.iptc_plus_dmi.is_some() {
            self.iptc_plus += 1;
        }
        if s.c2pa_training_mining.is_some() {
            self.c2pa += 1;
        }
        if s.rsl_permits.is_some() {
            self.rsl += 1;
        }
        if s.liccium_tdmai_iscc.is_some() || s.liccium_tdmai_allow.is_some() {
            self.liccium += 1;
        }
        if s.cloudflare_ai_train.is_some() {
            self.cloudflare += 1;
        }

        if let Some(spdx) = &e.license_spdx {
            let entry = self.license_by_spdx.entry(spdx.clone()).or_insert((0, 0));
            entry.0 += e.size_bytes;
            entry.1 += 1;
        }
    }

    fn finish(self) -> PredicateAggregates {
        // Compute the root here so the ~3.1 GB leaf vector is dropped before
        // the rest of the predicate is built.
        let merkle_root_bytes = merkle_root(&self.leaves);

        let signal_coverage = if self.row_count == 0 {
            SignalCoverage::default()
        } else {
            let total = self.row_count;
            SignalCoverage {
                robots_txt: Some(ppm(self.robots_txt, total)),
                ai_txt: Some(ppm(self.ai_txt, total)),
                tdm_rep: Some(ppm(self.tdm_rep, total)),
                aipref: Some(ppm(self.aipref, total)),
                iptc_plus: Some(ppm(self.iptc_plus, total)),
                c2pa: Some(ppm(self.c2pa, total)),
                rsl: Some(ppm(self.rsl, total)),
                liccium: Some(ppm(self.liccium, total)),
                cloudflare: Some(ppm(self.cloudflare, total)),
            }
        };

        let license_inventory = self
            .license_by_spdx
            .into_iter()
            .map(|(spdx_id, (byte_count, row_count))| LicenseInventoryEntry {
                spdx_id,
                byte_count,
                row_count: Some(row_count),
                notes: None,
            })
            .collect();

        PredicateAggregates {
            merkle_root: merkle_root_bytes,
            row_count: self.row_count,
            signal_coverage,
            license_inventory,
        }
    }
}

/// Stream a Parquet manifest through the constant-memory [`ManifestBatchReader`],
/// folding each row into a [`PredicateAggregator`] — never materializing the
/// whole `Vec<ManifestEntry>`. Errors surface as strings; the caller maps them
/// to the `ReadSchemaMismatch` transition (matching the prior `read_manifest`
/// error arm).
fn stream_predicate_aggregates(path: &Path) -> Result<PredicateAggregates, String> {
    let reader = ManifestBatchReader::open(path).map_err(|e| e.to_string())?;
    let mut acc = PredicateAggregator::default();
    for batch in reader {
        for entry in batch.map_err(|e| e.to_string())? {
            acc.add(&entry);
        }
    }
    Ok(acc.finish())
}

fn ppm(count: u64, total: u64) -> u32 {
    // total is guaranteed > 0 by caller. Defensive .min in case the
    // arithmetic ever overflows the 0..=1_000_000 range.
    ((count.saturating_mul(1_000_000) / total).min(1_000_000)) as u32
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

// ============================================================================
// File hashing
// ============================================================================

/// Compute BLAKE3 + SHA-256 of a file by STREAMING it in fixed-size chunks
/// (`attestrum_cas::stream_hash_path`) rather than reading the whole file into
/// memory — the ~8 GB merged 100BT manifest would otherwise spike RAM on a
/// 16 GB runner. BLAKE3 and SHA-256 are streaming-invariant, so the hex
/// digests are identical to a one-shot hash. Feeds both the in-toto
/// Subject.digest and the predicate's `manifest.digest_set`.
fn hash_file_blake3_sha256(path: &Path) -> std::io::Result<DigestMap> {
    let sh = stream_hash_path(path)?;
    Ok(DigestMap {
        blake3: hex_64(&sh.blake3),
        sha256: hex_64(&sh.sha256),
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

    // ---- Test oracles -------------------------------------------------------
    // The prior slice-based implementations, retained here as the reference the
    // streaming `PredicateAggregator` must match byte-for-byte. The four
    // pre-existing oracle tests below pin their behavior; the differential test
    // `streaming_aggregator_matches_slice_oracles` ties the aggregator to them.

    /// Per-signal PPM coverage. Evaluated-iff per signal: robots_txt =
    /// `robots_disallow || robots_user_agent.is_some()`; ai_txt =
    /// `ai_txt_disallow`; tdm_rep = `tdmrep_reservation != 0 ||
    /// tdmrep_policy_url.is_some()`; aipref/iptc_plus/c2pa/rsl/cloudflare =
    /// their respective `*.is_some()`; liccium = `liccium_tdmai_iscc.is_some()
    /// || liccium_tdmai_allow.is_some()`.
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

    /// The key determinism test: the streaming `PredicateAggregator` must
    /// produce the SAME root, count, signal coverage, and license inventory as
    /// the slice oracles — fed in several chunkings to prove batch-boundary
    /// invariance (the streaming reader yields 8192-row batches in production).
    #[test]
    fn streaming_aggregator_matches_slice_oracles() {
        let mut rows = vec![
            sample_entry(0x01, Some("MIT")),
            sample_entry(0x02, Some("Apache-2.0")),
            sample_entry(0x03, Some("MIT")),
            sample_entry(0x04, None),
            sample_entry(0x05, Some("Apache-2.0")),
            sample_entry(0x06, Some("MIT")),
            sample_entry(0x07, None),
        ];
        rows[0].signals.robots_disallow = true;
        rows[1].signals.ai_txt_disallow = true;
        rows[2].signals.tdmrep_reservation = 1;
        rows[3].signals.cloudflare_ai_train = Some("no".into());
        rows[4].signals.rsl_permits = Some("nothing".into());
        // Vary size_bytes so the license byte_count sums are exercised.
        for (i, r) in rows.iter_mut().enumerate() {
            r.size_bytes = 10 * (i as u64 + 1);
        }

        let want_coverage = signal_coverage_ppm(&rows);
        let want_inventory = aggregate_license_inventory(&rows);
        let want_inv: Vec<_> = want_inventory
            .iter()
            .map(|e| (e.spdx_id.clone(), e.byte_count, e.row_count))
            .collect();
        let want_root = merkle_root(&rows.iter().map(|r| r.document_id).collect::<Vec<_>>());
        let want_count = rows.len() as u64;

        for chunk_size in [1usize, 2, 3, 7, 100] {
            let mut acc = PredicateAggregator::default();
            for chunk in rows.chunks(chunk_size) {
                for e in chunk {
                    acc.add(e);
                }
            }
            let got = acc.finish();
            assert_eq!(got.row_count, want_count, "row_count (chunk {chunk_size})");
            assert_eq!(
                got.merkle_root, want_root,
                "merkle_root (chunk {chunk_size})"
            );
            assert_eq!(
                got.signal_coverage, want_coverage,
                "signal_coverage (chunk {chunk_size})"
            );
            let got_inv: Vec<_> = got
                .license_inventory
                .iter()
                .map(|e| (e.spdx_id.clone(), e.byte_count, e.row_count))
                .collect();
            assert_eq!(got_inv, want_inv, "license_inventory (chunk {chunk_size})");
        }
    }

    /// An empty manifest streams to the default (all-`None`) coverage, an empty
    /// inventory, and the empty-tree root — matching the prior `rows.is_empty()`
    /// guards.
    #[test]
    fn streaming_aggregator_empty_matches_oracles() {
        let got = PredicateAggregator::default().finish();
        assert_eq!(got.row_count, 0);
        assert_eq!(got.signal_coverage, signal_coverage_ppm(&[]));
        assert!(got.license_inventory.is_empty());
        assert_eq!(got.merkle_root, merkle_root(&[]));
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
