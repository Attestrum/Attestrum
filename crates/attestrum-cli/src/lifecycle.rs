//! Pure state machine for the `attestrum inspect` subcommand. Mirrors the
//! `docs/diagrams/sprint-3/attestrum-inspect-lifecycle.md` `stateDiagram-v2`
//! exactly. No I/O lives here — only the state graph and the
//! [`transition`] function that drives it. The integration tests at
//! `tests/inspect_proptest.rs` exercise this module directly to close
//! the SECOND `stateDiagram-v2` → proptest obligation per CLAUDE.md
//! §7.1 (first was Sprint 2 E2 for `signal-decision.md`).
//!
//! The real `commands::inspect::run` consumes [`transition`] to drive
//! its own state alongside actual file I/O, so both the spec and the
//! implementation share one source of truth.

/// Exit code categories surfaced by `attestrum inspect` (Sprint 3 E6) and
/// `attestrum sign` (Sprint 4 E3.5). Numeric codes per BUILD-PLAN §8.4 +
/// PATH-A-BRIEF §5.2. Reused across both subcommands so the lifecycle
/// state machines share one terminal value space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitCode {
    /// Exit 0 — success.
    Ok,
    /// Exit 1 — runtime error (Parquet I/O failure, unexpected file
    /// shape that isn't a schema-version mismatch, bundle write I/O).
    RuntimeError,
    /// Exit 2 — argument-style error (clap parse failure OR manifest
    /// path missing / not a file, per each diagram's literal contract).
    ArgsError,
    /// Exit 3 — `--offline` violation. Subcommand requires network but
    /// `--offline` was set (PATH-A-BRIEF §5.2). E3.5: `attestrum sign` exits
    /// 3 immediately if `--offline` is present.
    OfflineViolation,
    /// Exit 4 — signing identity error: OIDC token missing / invalid /
    /// expired, Fulcio rejected the CSR, certificate not yet valid at
    /// issuance time (PATH-A-BRIEF §5.2).
    IdentityError,
    /// Exit 5 — network error: Fulcio unreachable, Rekor unreachable,
    /// TUF trusted-root refresh failed (PATH-A-BRIEF §5.2).
    NetworkError,
    /// Exit 6 — cryptographic verification failure: cert chain invalid,
    /// signature mismatch, Rekor inclusion proof bad, RFC3161 timestamp
    /// outside cert validity window, OR the extracted identity does not
    /// satisfy the operator-supplied regex policy (PATH-A-BRIEF §5.2).
    /// Sprint 4 E4 addition; surfaced only by `attestrum verify`.
    VerificationFailure,
    /// Exit 8 — schema validation failure. For `attestrum inspect`:
    /// manifest's `attestrum.manifest.schema_version` KeyValue does not
    /// match `attestrum_manifest::SCHEMA_VERSION`. Reserved by `attestrum sign`'s
    /// lifecycle as the documented Exit 8 slot for future predicate-
    /// JSON-Schema-drift detection; no E3.5 code path emits it (the
    /// Rust predicate types ARE the schema via schemars derive, so
    /// drift surface is zero at the moment of sign).
    SchemaError,
}

impl ExitCode {
    /// Numeric exit code for the process. Stable across releases —
    /// changing this would break shell scripts that check `$?`.
    pub fn as_u8(self) -> u8 {
        match self {
            ExitCode::Ok => 0,
            ExitCode::RuntimeError => 1,
            ExitCode::ArgsError => 2,
            ExitCode::OfflineViolation => 3,
            ExitCode::IdentityError => 4,
            ExitCode::NetworkError => 5,
            ExitCode::VerificationFailure => 6,
            ExitCode::SchemaError => 8,
        }
    }
}

/// States the `attestrum inspect` lifecycle visits between invocation and
/// process exit. Mirrors the diagram one-to-one. `Exit(ExitCode)`
/// collapses the four terminal `Exit0` / `Exit1` / `Exit2` / `Exit8`
/// state nodes into a single value-carrying variant — equality on the
/// inner code is what the proptest asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectState {
    Invoked,
    ArgsParsed,
    Validated,
    LocalRead,
    ManifestLoaded,
    Summarized,
    Exit(ExitCode),
}

impl InspectState {
    /// True iff this is one of the four `Exit(...)` terminal states.
    pub fn is_terminal(self) -> bool {
        matches!(self, InspectState::Exit(_))
    }
}

/// Events that drive the lifecycle forward. Each event corresponds to
/// exactly one outgoing edge in the diagram. Undocumented (state,
/// event) pairs hold the current state (no silent forward progress)
/// per the proptest property `proptest_no_undocumented_transition_is_taken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectEvent {
    /// Invoked → ArgsParsed.
    ClapParseOk,
    /// Invoked → Exit(ArgsError).
    ClapParseError,
    /// ArgsParsed → Validated.
    PathExistsAndIsFile,
    /// ArgsParsed → Exit(ArgsError).
    PathMissingOrNotFile,
    /// Validated → LocalRead.
    DispatchInspect,
    /// LocalRead → ManifestLoaded.
    ReadOk,
    /// LocalRead → Exit(RuntimeError).
    ReadIoError,
    /// LocalRead → Exit(SchemaError).
    ReadSchemaMismatch,
    /// ManifestLoaded → Summarized.
    ComputeSummary,
    /// Summarized → Exit(Ok).
    PrintSummary,
}

/// Drive one transition. For (state, event) pairs documented in the
/// diagram, returns the diagram's target state. For undocumented pairs
/// — including any event from a terminal `Exit(...)` state — returns
/// the input state unchanged (no silent forward progress).
pub fn transition(state: InspectState, event: InspectEvent) -> InspectState {
    use ExitCode::*;
    use InspectEvent::*;
    use InspectState::*;
    match (state, event) {
        (Invoked, ClapParseOk) => ArgsParsed,
        (Invoked, ClapParseError) => Exit(ArgsError),
        (ArgsParsed, PathExistsAndIsFile) => Validated,
        (ArgsParsed, PathMissingOrNotFile) => Exit(ArgsError),
        (Validated, DispatchInspect) => LocalRead,
        (LocalRead, ReadOk) => ManifestLoaded,
        (LocalRead, ReadIoError) => Exit(RuntimeError),
        (LocalRead, ReadSchemaMismatch) => Exit(SchemaError),
        (ManifestLoaded, ComputeSummary) => Summarized,
        (Summarized, PrintSummary) => Exit(Ok),
        // Undocumented pair: hold. The diagram pins this as the design
        // choice for `proptest_no_undocumented_transition_is_taken`.
        (s, _) => s,
    }
}

/// The full list of documented `(from_state, event, to_state)` triples
/// the diagram declares. The proptest suite enumerates this set to
/// prove every diagram edge is reachable from [`transition`], and to
/// distinguish documented from undocumented event firings.
///
/// Order matches top-to-bottom in `docs/diagrams/sprint-3/attestrum-inspect-lifecycle.md`.
pub fn documented_transitions() -> &'static [(InspectState, InspectEvent, InspectState)] {
    use ExitCode::*;
    use InspectEvent::*;
    use InspectState::*;
    &[
        (Invoked, ClapParseOk, ArgsParsed),
        (Invoked, ClapParseError, Exit(ArgsError)),
        (ArgsParsed, PathExistsAndIsFile, Validated),
        (ArgsParsed, PathMissingOrNotFile, Exit(ArgsError)),
        (Validated, DispatchInspect, LocalRead),
        (LocalRead, ReadOk, ManifestLoaded),
        (LocalRead, ReadIoError, Exit(RuntimeError)),
        (LocalRead, ReadSchemaMismatch, Exit(SchemaError)),
        (ManifestLoaded, ComputeSummary, Summarized),
        (Summarized, PrintSummary, Exit(Ok)),
    ]
}

/// Every event variant. Used by the proptest's arbitrary-event
/// generator to sample uniformly from the full event space.
pub fn all_events() -> &'static [InspectEvent] {
    use InspectEvent::*;
    &[
        ClapParseOk,
        ClapParseError,
        PathExistsAndIsFile,
        PathMissingOrNotFile,
        DispatchInspect,
        ReadOk,
        ReadIoError,
        ReadSchemaMismatch,
        ComputeSummary,
        PrintSummary,
    ]
}

/// Every non-terminal state variant. Used by the proptest to sample
/// starting states for undocumented-event hold checks.
pub fn all_non_terminal_states() -> &'static [InspectState] {
    use InspectState::*;
    &[
        Invoked,
        ArgsParsed,
        Validated,
        LocalRead,
        ManifestLoaded,
        Summarized,
    ]
}

// ============================================================================
// Sprint 4 E3.5 — `attestrum sign` lifecycle
// ============================================================================
//
// Mirrors `docs/diagrams/sprint-4/sign-flow.md` (sequenceDiagram, flipped
// to `source_of_truth: code` in the same commit that ships this file). The
// contract-test obligation per PATH-A-BRIEF §7.1 lives at
// `crates/attestrum-cli/tests/sign_flow_contract.rs`. Pure code, no I/O — the
// concrete consumer is `crate::commands::sign::run`.
//
// The state space is wider than `attestrum inspect`'s because `attestrum sign`
// touches the network (Fulcio + Rekor + TUF), the OIDC token sourcing
// layer (env / file), and the OIDC-violation guard (`--offline`). Exit
// codes 3/4/5 are new for this subcommand per PATH-A-BRIEF §5.2.

/// States the `attestrum sign` lifecycle visits between invocation and
/// process exit. Mirrors the diagram one-to-one. `Exit(ExitCode)` collapses
/// the terminal Exit nodes into a single value-carrying variant — equality
/// on the inner code is what the contract test asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignState {
    Invoked,
    ArgsParsed,
    Validated,
    OfflineCheck,
    OidcResolved,
    ManifestLoaded,
    PredicateBuilt,
    StatementBuilt,
    PayloadCanonicalized,
    SignerOpened,
    Signed,
    BundleWritten,
    Exit(ExitCode),
}

impl SignState {
    /// True iff this is a terminal `Exit(...)` state.
    pub fn is_terminal(self) -> bool {
        matches!(self, SignState::Exit(_))
    }
}

/// Events that drive the sign lifecycle forward. Each event corresponds
/// to exactly one outgoing edge in `sign-flow.md`. Undocumented (state,
/// event) pairs hold the current state (no silent forward progress) per
/// the contract test property `no_undocumented_transition_is_taken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignEvent {
    /// Invoked → ArgsParsed.
    ClapParseOk,
    /// Invoked → Exit(ArgsError). Clap fires exit 2 before `run` returns;
    /// included for symmetry with InspectEvent::ClapParseError.
    ClapParseError,
    /// ArgsParsed → Validated.
    PathExistsAndIsFile,
    /// ArgsParsed → Exit(ArgsError).
    PathMissingOrNotFile,
    /// Validated → OfflineCheck.
    DispatchSign,
    /// OfflineCheck → Exit(OfflineViolation).
    OfflineFlag,
    /// OfflineCheck → OidcResolved.
    OidcTokenLoaded,
    /// OfflineCheck → Exit(IdentityError). Token absent (no
    /// `--oidc-token-file`, no `SIGSTORE_ID_TOKEN` env var).
    OidcTokenMissing,
    /// OidcResolved → ManifestLoaded.
    ReadOk,
    /// OidcResolved → Exit(RuntimeError). Parquet I/O failure that
    /// isn't a schema-version mismatch.
    ReadIoError,
    /// OidcResolved → Exit(SchemaError). Manifest's
    /// `attestrum.manifest.schema_version` KeyValue does not match
    /// `attestrum_manifest::SCHEMA_VERSION`.
    ReadSchemaMismatch,
    /// ManifestLoaded → PredicateBuilt.
    BuildPredicate,
    /// PredicateBuilt → StatementBuilt.
    BuildStatement,
    /// StatementBuilt → PayloadCanonicalized.
    Canonicalize,
    /// PayloadCanonicalized → SignerOpened. TUF root + Fulcio session
    /// initialised successfully.
    SigningContextOk,
    /// PayloadCanonicalized → Exit(NetworkError). TUF refresh failed
    /// or Fulcio session init failed for network reasons.
    SigningContextFail,
    /// SignerOpened → Signed. DSSE-sign + Rekor v2 submit succeeded.
    SignOk,
    /// SignerOpened → Exit(IdentityError). OIDC token rejected by
    /// Fulcio, cert validity bad, identity-binding mismatch.
    SignIdentityError,
    /// SignerOpened → Exit(NetworkError). Rekor unreachable mid-sign.
    SignNetworkError,
    /// Signed → BundleWritten.
    WriteOk,
    /// Signed → Exit(RuntimeError). Bundle file I/O failed.
    WriteIoError,
    /// BundleWritten → Exit(Ok).
    PrintSummary,
}

/// Drive one transition. For (state, event) pairs documented in the
/// diagram, returns the diagram's target state. For undocumented pairs
/// — including any event from a terminal `Exit(...)` state — returns
/// the input state unchanged.
pub fn sign_transition(state: SignState, event: SignEvent) -> SignState {
    use ExitCode::*;
    use SignEvent::*;
    use SignState::*;
    match (state, event) {
        (Invoked, ClapParseOk) => ArgsParsed,
        (Invoked, ClapParseError) => Exit(ArgsError),
        (ArgsParsed, PathExistsAndIsFile) => Validated,
        (ArgsParsed, PathMissingOrNotFile) => Exit(ArgsError),
        (Validated, DispatchSign) => OfflineCheck,
        (OfflineCheck, OfflineFlag) => Exit(OfflineViolation),
        (OfflineCheck, OidcTokenLoaded) => OidcResolved,
        (OfflineCheck, OidcTokenMissing) => Exit(IdentityError),
        (OidcResolved, ReadOk) => ManifestLoaded,
        (OidcResolved, ReadIoError) => Exit(RuntimeError),
        (OidcResolved, ReadSchemaMismatch) => Exit(SchemaError),
        (ManifestLoaded, BuildPredicate) => PredicateBuilt,
        (PredicateBuilt, BuildStatement) => StatementBuilt,
        (StatementBuilt, Canonicalize) => PayloadCanonicalized,
        (PayloadCanonicalized, SigningContextOk) => SignerOpened,
        (PayloadCanonicalized, SigningContextFail) => Exit(NetworkError),
        (SignerOpened, SignOk) => Signed,
        (SignerOpened, SignIdentityError) => Exit(IdentityError),
        (SignerOpened, SignNetworkError) => Exit(NetworkError),
        (Signed, WriteOk) => BundleWritten,
        (Signed, WriteIoError) => Exit(RuntimeError),
        (BundleWritten, PrintSummary) => Exit(Ok),
        // Undocumented pair: hold. Locks the contract-test property
        // `no_undocumented_transition_is_taken`.
        (s, _) => s,
    }
}

/// The full list of documented `(from_state, event, to_state)` triples
/// the diagram declares. Top-to-bottom in `docs/diagrams/sprint-4/sign-flow.md`.
pub fn sign_documented_transitions() -> &'static [(SignState, SignEvent, SignState)] {
    use ExitCode::*;
    use SignEvent::*;
    use SignState::*;
    &[
        (Invoked, ClapParseOk, ArgsParsed),
        (Invoked, ClapParseError, Exit(ArgsError)),
        (ArgsParsed, PathExistsAndIsFile, Validated),
        (ArgsParsed, PathMissingOrNotFile, Exit(ArgsError)),
        (Validated, DispatchSign, OfflineCheck),
        (OfflineCheck, OfflineFlag, Exit(OfflineViolation)),
        (OfflineCheck, OidcTokenLoaded, OidcResolved),
        (OfflineCheck, OidcTokenMissing, Exit(IdentityError)),
        (OidcResolved, ReadOk, ManifestLoaded),
        (OidcResolved, ReadIoError, Exit(RuntimeError)),
        (OidcResolved, ReadSchemaMismatch, Exit(SchemaError)),
        (ManifestLoaded, BuildPredicate, PredicateBuilt),
        (PredicateBuilt, BuildStatement, StatementBuilt),
        (StatementBuilt, Canonicalize, PayloadCanonicalized),
        (PayloadCanonicalized, SigningContextOk, SignerOpened),
        (PayloadCanonicalized, SigningContextFail, Exit(NetworkError)),
        (SignerOpened, SignOk, Signed),
        (SignerOpened, SignIdentityError, Exit(IdentityError)),
        (SignerOpened, SignNetworkError, Exit(NetworkError)),
        (Signed, WriteOk, BundleWritten),
        (Signed, WriteIoError, Exit(RuntimeError)),
        (BundleWritten, PrintSummary, Exit(Ok)),
    ]
}

/// Every SignEvent variant. Used by the contract test's arbitrary-event
/// generator to sample uniformly from the full event space.
pub fn sign_all_events() -> &'static [SignEvent] {
    use SignEvent::*;
    &[
        ClapParseOk,
        ClapParseError,
        PathExistsAndIsFile,
        PathMissingOrNotFile,
        DispatchSign,
        OfflineFlag,
        OidcTokenLoaded,
        OidcTokenMissing,
        ReadOk,
        ReadIoError,
        ReadSchemaMismatch,
        BuildPredicate,
        BuildStatement,
        Canonicalize,
        SigningContextOk,
        SigningContextFail,
        SignOk,
        SignIdentityError,
        SignNetworkError,
        WriteOk,
        WriteIoError,
        PrintSummary,
    ]
}

/// Every non-terminal SignState variant. Used by the contract test to
/// sample starting states for undocumented-event hold checks.
pub fn sign_all_non_terminal_states() -> &'static [SignState] {
    use SignState::*;
    &[
        Invoked,
        ArgsParsed,
        Validated,
        OfflineCheck,
        OidcResolved,
        ManifestLoaded,
        PredicateBuilt,
        StatementBuilt,
        PayloadCanonicalized,
        SignerOpened,
        Signed,
        BundleWritten,
    ]
}

// ============================================================================
// Sprint 4 E4 — `attestrum verify` lifecycle
// ============================================================================
//
// Mirrors `docs/diagrams/sprint-4/verify-flow.md` (sequenceDiagram, flipped
// to `source_of_truth: code` in the same commit that ships this file). The
// contract-test obligation per PATH-A-BRIEF §7.1 lives at
// `crates/attestrum-cli/tests/verify_flow_contract.rs`. Pure code, no I/O.
//
// Mirrors the SignState shape (Sprint 4 E3.5). Exit codes 0, 1, 2, 3, 5,
// 6, 8 per PATH-A-BRIEF §5.2 — no Exit 4 (verify side doesn't surface
// OIDC token errors since it doesn't sign). Exit 6 is new for the verify
// side: cryptographic verification failure (cert chain, signature, Rekor,
// TSA, OR identity-regex mismatch).

/// States the `attestrum verify` lifecycle visits between invocation and
/// process exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifyState {
    Invoked,
    ArgsParsed,
    Validated,
    BundleLoaded,
    IdentityExtracted,
    TrustRootResolved,
    CryptoVerified,
    IdentityChecked,
    StatementExtracted,
    SchemaValidated,
    Exit(ExitCode),
}

impl VerifyState {
    pub fn is_terminal(self) -> bool {
        matches!(self, VerifyState::Exit(_))
    }
}

/// Events that drive the verify lifecycle forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifyEvent {
    /// Invoked → ArgsParsed.
    ClapParseOk,
    /// Invoked → Exit(ArgsError). Symmetry with Sign/Inspect.
    ClapParseError,
    /// ArgsParsed → Validated.
    PathsExistAndAreFiles,
    /// ArgsParsed → Exit(ArgsError).
    PathMissingOrNotFile,
    /// Validated → BundleLoaded.
    BundleReadOk,
    /// Validated → Exit(RuntimeError). File I/O on the bundle path.
    BundleReadIoError,
    /// BundleLoaded → IdentityExtracted.
    IdentityExtractOk,
    /// BundleLoaded → Exit(VerificationFailure). Malformed cert / no SAN
    /// / no Fulcio OID extension.
    IdentityExtractFailed,
    /// IdentityExtracted → TrustRootResolved (TUF refresh or cached read OK).
    TrustRootOk,
    /// IdentityExtracted → Exit(OfflineViolation). `--offline` set + cache
    /// stale beyond TUF freshness window.
    OfflineWithStaleCache,
    /// IdentityExtracted → Exit(NetworkError). TUF refresh failed for
    /// network reasons (`--offline` not set).
    TufRefreshFail,
    /// TrustRootResolved → CryptoVerified. sigstore-rs's verify() returned ok.
    SigstoreVerifyOk,
    /// TrustRootResolved → Exit(VerificationFailure). sigstore-rs rejected
    /// the bundle (cert chain, sig, Rekor inclusion proof, RFC3161 TSA).
    SigstoreVerifyFail,
    /// CryptoVerified → IdentityChecked. Regex matched extracted identity.
    IdentityRegexMatchOk,
    /// CryptoVerified → Exit(VerificationFailure). Regex did not match.
    IdentityRegexMismatch,
    /// IdentityChecked → StatementExtracted. DSSE payload base64-decoded
    /// + parsed as in-toto Statement.
    PayloadDecodeOk,
    /// IdentityChecked → Exit(RuntimeError). Payload base64 or JSON parse
    /// failure (bundle malformed in a way sigstore-rs should have caught).
    PayloadDecodeFail,
    /// StatementExtracted → SchemaValidated. Predicate deserialised as
    /// TrainingCorpusPredicate (light-weight Exit 8 path).
    PredicateDeserializeOk,
    /// StatementExtracted → Exit(SchemaError). Predicate doesn't satisfy
    /// the v0.3 schema (deserialise failure).
    PredicateDeserializeFail,
    /// SchemaValidated → Exit(Ok).
    PrintSummary,
}

/// Drive one transition. Undocumented pairs hold (no silent forward progress).
pub fn verify_transition(state: VerifyState, event: VerifyEvent) -> VerifyState {
    use ExitCode::*;
    use VerifyEvent::*;
    use VerifyState::*;
    match (state, event) {
        (Invoked, ClapParseOk) => ArgsParsed,
        (Invoked, ClapParseError) => Exit(ArgsError),
        (ArgsParsed, PathsExistAndAreFiles) => Validated,
        (ArgsParsed, PathMissingOrNotFile) => Exit(ArgsError),
        (Validated, BundleReadOk) => BundleLoaded,
        (Validated, BundleReadIoError) => Exit(RuntimeError),
        (BundleLoaded, IdentityExtractOk) => IdentityExtracted,
        (BundleLoaded, IdentityExtractFailed) => Exit(VerificationFailure),
        (IdentityExtracted, TrustRootOk) => TrustRootResolved,
        (IdentityExtracted, OfflineWithStaleCache) => Exit(OfflineViolation),
        (IdentityExtracted, TufRefreshFail) => Exit(NetworkError),
        (TrustRootResolved, SigstoreVerifyOk) => CryptoVerified,
        (TrustRootResolved, SigstoreVerifyFail) => Exit(VerificationFailure),
        (CryptoVerified, IdentityRegexMatchOk) => IdentityChecked,
        (CryptoVerified, IdentityRegexMismatch) => Exit(VerificationFailure),
        (IdentityChecked, PayloadDecodeOk) => StatementExtracted,
        (IdentityChecked, PayloadDecodeFail) => Exit(RuntimeError),
        (StatementExtracted, PredicateDeserializeOk) => SchemaValidated,
        (StatementExtracted, PredicateDeserializeFail) => Exit(SchemaError),
        (SchemaValidated, PrintSummary) => Exit(Ok),
        // Undocumented pair: hold.
        (s, _) => s,
    }
}

/// Documented `(from_state, event, to_state)` triples for `verify-flow.md`.
pub fn verify_documented_transitions() -> &'static [(VerifyState, VerifyEvent, VerifyState)] {
    use ExitCode::*;
    use VerifyEvent::*;
    use VerifyState::*;
    &[
        (Invoked, ClapParseOk, ArgsParsed),
        (Invoked, ClapParseError, Exit(ArgsError)),
        (ArgsParsed, PathsExistAndAreFiles, Validated),
        (ArgsParsed, PathMissingOrNotFile, Exit(ArgsError)),
        (Validated, BundleReadOk, BundleLoaded),
        (Validated, BundleReadIoError, Exit(RuntimeError)),
        (BundleLoaded, IdentityExtractOk, IdentityExtracted),
        (
            BundleLoaded,
            IdentityExtractFailed,
            Exit(VerificationFailure),
        ),
        (IdentityExtracted, TrustRootOk, TrustRootResolved),
        (
            IdentityExtracted,
            OfflineWithStaleCache,
            Exit(OfflineViolation),
        ),
        (IdentityExtracted, TufRefreshFail, Exit(NetworkError)),
        (TrustRootResolved, SigstoreVerifyOk, CryptoVerified),
        (
            TrustRootResolved,
            SigstoreVerifyFail,
            Exit(VerificationFailure),
        ),
        (CryptoVerified, IdentityRegexMatchOk, IdentityChecked),
        (
            CryptoVerified,
            IdentityRegexMismatch,
            Exit(VerificationFailure),
        ),
        (IdentityChecked, PayloadDecodeOk, StatementExtracted),
        (IdentityChecked, PayloadDecodeFail, Exit(RuntimeError)),
        (StatementExtracted, PredicateDeserializeOk, SchemaValidated),
        (
            StatementExtracted,
            PredicateDeserializeFail,
            Exit(SchemaError),
        ),
        (SchemaValidated, PrintSummary, Exit(Ok)),
    ]
}

pub fn verify_all_events() -> &'static [VerifyEvent] {
    use VerifyEvent::*;
    &[
        ClapParseOk,
        ClapParseError,
        PathsExistAndAreFiles,
        PathMissingOrNotFile,
        BundleReadOk,
        BundleReadIoError,
        IdentityExtractOk,
        IdentityExtractFailed,
        TrustRootOk,
        OfflineWithStaleCache,
        TufRefreshFail,
        SigstoreVerifyOk,
        SigstoreVerifyFail,
        IdentityRegexMatchOk,
        IdentityRegexMismatch,
        PayloadDecodeOk,
        PayloadDecodeFail,
        PredicateDeserializeOk,
        PredicateDeserializeFail,
        PrintSummary,
    ]
}

pub fn verify_all_non_terminal_states() -> &'static [VerifyState] {
    use VerifyState::*;
    &[
        Invoked,
        ArgsParsed,
        Validated,
        BundleLoaded,
        IdentityExtracted,
        TrustRootResolved,
        CryptoVerified,
        IdentityChecked,
        StatementExtracted,
        SchemaValidated,
    ]
}
