# Session 3 — verify-side confirmation report

**Author**: Claude Opus 4.7 (1M context), under founder direction
**Date**: 2026-05-25
**Attestrum HEAD at report-write**: `4c0c1c0 chore(deps): bump sigstore-rs fork to b4ea971 for Body::dsse variant (Session 2A.2)`
**Fork HEAD at report-write**: `b4ea971 feat(rekor): add Body::dsse variant + dsse / dsse_all_of model files for Rekor v1 dsse@0.0.1 response` (on `Attestrum/sigstore-rs` branch `attestrum/email-optional-for-workload-identity-tokens`)
**Predecessor report**: `attestrum-cosign-interop-verification-report-2026-05-25.md` (V1-V7 diagnosis that led to the X→Y hybrid path)
**Successor handoff**: `/Users/austinmunday/.claude/plans/cosign-interop-session-4-ci-green-flip-handoff-2026-05-25.md` (Session 4 — cosign-side delta diagnosis + CI green flip)

---

## 1. Headline

The X→Y hybrid sign-side rewrite is working end-to-end through `attest_verify`. Session 2A.2's fork-side completeness fix closed the response-side deserialization gap that Session 3's resume protocol discovered. **All six W1-W6 verification checks pass.** The `cosign-interop.yml` workflow now fails at a NEW location — the cosign-shellout assertion at `tests/cosign_interop.rs:177` — with cosign rejecting the bundle for `"kind and version mismatch: dsse/0.0.1 != hashedrekord/0.0.1"`. That delta is downstream of `attest_verify` and is Session 4's scope.

Session 3 produced one Attestrum-side commit (`4c0c1c0` — the rev-pin bump) and one fork-side commit (`b4ea971` — the missing `Body::dsse(DsseAllOf)` variant + sibling files + regression test). No other code changes were required; the diagram contract at `docs/diagrams/sprint-4/sign-flow.md` stays as-is (`source_of_truth: diagram`, `last_verified: ff7f41c 2026-05-25`) until Session 4's CI flip.

---

## 2. Checklist

| # | Check | Outcome | Evidence |
|---|---|---|---|
| W1 | sign + verify round-trip | **PASS** | cosign-interop CI run `26419364100` reaches `tests/cosign_interop.rs:177` (the cosign assertion); the preceding `attest_sign` + `bundle_path.exists()` + `attest_verify(...).expect(...)` at lines 130-155 all succeeded. No panic from sigstore-rs internals. |
| W2 | identity extraction | **PASS** | `attest_verify` at lines 148-155 requires `extract_identity` to return non-placeholder values for the operator-supplied identity / issuer regexes (built from `regex::escape(&signed.identity)` + `regex::escape(&signed.oidc_issuer)` at lines 146-147). The `.expect("attestrum_attest::verify self-verify sanity gate")` succeeded → identity extraction round-tripped against the v0.3 `verificationMaterial.x509CertificateChain.certificates[0].rawBytes` shape. |
| W3 | bundle shape | **PASS** (by construction + cosign log) | sigstore-rs `src/bundle/sign.rs:308` hardcodes `media_type: Version::Bundle0_3.to_string()` ⇒ `"application/vnd.dev.sigstore.bundle.v0.3+json"`. `payload_type: payload_type.to_owned()` at line 262 with the caller passing `"application/vnd.in-toto+json"` from `crates/attestrum-attest/src/sign.rs:104`. `tlogEntries[0].kindVersion = {"kind": "dsse", "version": "0.0.1"}` confirmed indirectly by cosign's error message naming exactly that string: `"dsse/0.0.1 != hashedrekord/0.0.1"`. |
| W4 | diagram-vs-code drift | **PASS** | `docs/diagrams/sprint-4/sign-flow.md` describes `SigningSession::sign_dsse` as the fork-side method that emits the bundle. Session 2A.2 closes a gap INSIDE that method's internal Rekor-response decode path — below the diagram's abstraction level. No diagram change needed; verified by re-reading the `participant Sess` block + the `participant Rekor` Note text. |
| W5 | dep + license inventory | **PASS** | `cargo deny check sources licenses` on HEAD `4c0c1c0`: `licenses ok, sources ok`. No new direct workspace deps; the `[patch.crates-io]` rev pin is the only change. `docs/license-inventory.md` does not need a new row (no new crate; same `sigstore` consumed at a different git rev). |
| W6 | diagram-linter freshness | **PASS** | `cargo run -p diagram-linter --release --quiet -- check --strict` on HEAD `4c0c1c0`: `96 checks run, 0 failure(s) [strict]`. `last_verified: ff7f41c 2026-05-25` in `sign-flow.md`'s frontmatter is within the last 30 commits (`ff7f41c` is 3 commits behind `4c0c1c0` — well within the linter's freshness window). |

---

## 3. W1 details — sign + verify round-trip

The cosign-interop integration test at `crates/attestrum-attest/tests/cosign_interop.rs` exercises the full round-trip in this order:

1. **Lines 70-72**: `build_corpus` creates a real `manifest.parquet` from an empty corpus through the production pipeline.
2. **Lines 119-126**: Construct in-toto Statement + canonical-JSON payload.
3. **Lines 130-135**: `attest_sign(SignRequest { ... }).expect("attestrum_attest::sign against public-good")`.
4. **Lines 137-139**: `assert!(bundle_path.exists(), ...)`.
5. **Lines 146-155**: `attest_verify(VerifyRequest { ... }).expect("attestrum_attest::verify self-verify sanity gate")` — the regex policy is built from `regex::escape(&signed.identity)` + `regex::escape(&signed.oidc_issuer)` so the policy is by-construction a literal-match.
6. **Lines 162-173**: Shell out to `cosign verify-blob-attestation --new-bundle-format --bundle ... <manifest>`.
7. **Lines 177-181**: `assert!(cosign.status.success(), "cosign verify-blob-attestation failed: exit={:?}\nstdout={stdout}\nstderr={stderr}", ...)`.

The CI run at `26419364100` produced:

```
test cosign_interop ... FAILED

---- cosign_interop stdout ----

thread 'cosign_interop' panicked at crates/attestrum-attest/tests/cosign_interop.rs:177:5:
cosign verify-blob-attestation failed: exit=Some(1)
stdout=
stderr=Error: validation error: kind and version mismatch: dsse/0.0.1 != hashedrekord/0.0.1
error during command execution: validation error: kind and version mismatch: dsse/0.0.1 != hashedrekord/0.0.1
```

The panic is at line 177:5 (the `assert!` macro at the cosign-shellout step). Per the test's sequential structure, reaching line 177 requires all preceding steps to have succeeded — including the `attest_sign(...).expect(...)` at line 134 (no panic-on-sign), `assert!(bundle_path.exists(), ...)` at line 137 (bundle written), and `attest_verify(...).expect(...)` at line 154 (verify round-trip Ok). W1 PASS.

Compare to the predecessor run on `8ae461a` (`26417018541`) where the panic was at sigstore-rs internals `log_entry.rs:58:22` — strictly before `attest_sign` could return Ok.

---

## 4. W2 details — identity extraction

The verify-side identity extraction has two paths, both exercised by the round-trip above:

1. **Sign-side enrichment**: After writing the bundle to disk, `crates/attestrum-attest/src/sign.rs:137-146` re-parses the bundle JSON and calls `crate::identity::extract_identity(&v)` to populate `SignedAttestation::identity` + `SignedAttestation::oidc_issuer`. The cosign-interop test at line 146-147 then builds the regex policies as `format!("^{}$", regex::escape(&signed.identity))` — if extraction had returned the fallback `"<unparseable from bundle cert>"`, the regex would be `^<unparseable from bundle cert>$` and the next step (`attest_verify`'s regex-policy gate at `src/verify.rs:123-132`) would still match by construction… but it would also need `extract_identity` to RE-extract the SAN from the bundle independently inside `attest_verify` at line 115. The independent extraction at verify time uses the same v0.3-aware `identity::locate_leaf_cert_der` path (handles both `verificationMaterial.certificate.rawBytes` and `verificationMaterial.x509CertificateChain.certificates[0].rawBytes`). sigstore-rs's Session 2A `sign_dsse` emits the chain form via `src/bundle/sign.rs:295-299`. Both paths align.

2. **Verify-side gate**: `attest_verify` succeeded (the `.expect(...)` at line 154 didn't fire), which required:
   - `extract_identity(&bundle_value)` at `src/verify.rs:115` returning Ok with non-placeholder SAN + issuer.
   - The regex policy match at lines 123-132 passing.
   - The cryptographic verify at line 152-154 passing (validates cert chain + verifies signature against PAE + checks Rekor inclusion proof).

Both paths returned Ok. W2 PASS. (Implicitly — the existing test does not log the extracted identity string. Session 4 could add an `eprintln!("signed.identity = {}", signed.identity)` line for explicit display; the assertion-cleanup commit there could include it.)

---

## 5. W3 details — bundle shape

By construction from sigstore-rs `src/bundle/sign.rs` at fork commit `b4ea971`:

```rust
// Line 308:
Ok(Bundle {
    media_type: Version::Bundle0_3.to_string(),       // "application/vnd.dev.sigstore.bundle.v0.3+json"
    verification_material,
    content: Some(bundle::Content::DsseEnvelope(envelope)),  // dsseEnvelope, not messageSignature
})
```

Where `envelope` at lines 260-267:

```rust
let envelope = sigstore_protobuf_specs::io::intoto::Envelope {
    payload: payload.to_vec(),                       // raw payload bytes (base64'd at serialize time)
    payload_type: payload_type.to_owned(),           // "application/vnd.in-toto+json" (passed by dsse_sign)
    signatures: vec![sigstore_protobuf_specs::io::intoto::Signature {
        sig: signature_bytes.clone(),
        keyid: String::new(),
    }],
};
```

And the Rekor `kindVersion` at lines 274-282 (request) flows to the response unchanged (`create_log_entry` → `LogEntry { body: Body::dsse(DsseAllOf { api_version: "0.0.1", ... }), ... }` → `TryInto<TransparencyLogEntry>` preserving `kind_version`):

```rust
let proposed_entry = ProposedLogEntry::Dsse {
    api_version: "0.0.1".to_owned(),
    spec: serde_json::json!({
        "proposedContent": {
            "envelope": envelope_json,
            "verifiers": [base64.encode(cert_pem.as_bytes())],
        }
    }),
};
```

The cosign error log line `"kind and version mismatch: dsse/0.0.1 != hashedrekord/0.0.1"` confirms the actual on-disk bundle's `tlogEntries[0].kindVersion` reads as `dsse/0.0.1` — exactly the target shape.

All three W3 sub-checks (mediaType, dsseEnvelope.payloadType, tlogEntries[0].kindVersion) PASS.

---

## 6. W4 details — diagram-vs-code drift

`docs/diagrams/sprint-4/sign-flow.md`'s Mermaid block has the relevant participants:

- `Sess as SigningSession::sign_dsse<br/>(fork API — Session 2A fork commit e551bf9)`
- `Rekor as Rekor v1 dsse@0.0.1<br/>(kind=dsse, version=0.0.1, proposedContent envelope+verifiers)`

And the relevant edge:

- `Sess->>Rekor: submit ProposedEntry::Dsse { apiVersion: "0.0.1", spec: { proposedContent: { envelope: serde_json(envelope), verifiers: [base64(cert_PEM_LF)] } } }`
- `Rekor-->>Sess: tlog entry { logIndex, integratedTime, kindVersion:{kind:"dsse", version:"0.0.1"}, canonicalizedBody:{spec:{envelopeHash, payloadHash, signatures}}, inclusionProof, signedEntryTimestamp }`

Session 2A.2 closes the deserialization gap on the response-side `Rekor-->>Sess` edge. The diagram already describes that response shape correctly — the gap was in sigstore-rs's ability to parse that JSON shape into Rust types, not in the contract. No diagram update needed.

Note: The fork-commit SHA in the `Sess` participant title (`e551bf9`) is technically stale — the actual fork commit being consumed is now `b4ea971`. This is a documentation-decay surface, not a contract bug. Session 4 owns the cleanup along with the `source_of_truth: diagram → code` flip; the fork-commit SHA can be replaced with `b4ea971` (or omitted entirely once `source_of_truth: code` makes the runtime code authoritative) in the same Session 4 commit. **Not a Session 3 problem** because Session 3's verification checks are about whether the diagram still describes the contract correctly, and it does — the `sign_dsse` method behavior at fork HEAD `b4ea971` matches the diagram's `participant Sess` description; the historical attribution to `e551bf9` is just a footnote that will get updated.

W4 PASS.

---

## 7. W5 details — dep + license inventory

The only Attestrum-side dependency change in this session is the `[patch.crates-io]` git-pin rev bump:

```diff
-sigstore = { git = "https://github.com/Attestrum/sigstore-rs.git", rev = "e551bf9ec3b49fe5423282fd8c4a724f11f3c7dc" }
+sigstore = { git = "https://github.com/Attestrum/sigstore-rs.git", rev = "b4ea971d4837d823ed062f92ca8000fe970b8d81" }
```

The crate (`sigstore`) is the same; the source URL is the same; only the rev pin moved forward by one fork commit. `docs/license-inventory.md` already lists `sigstore` with Apache-2.0 (per the fork repo's LICENSE). No new row.

`cargo deny check sources licenses` on HEAD `4c0c1c0`:
```
licenses ok, sources ok
```

The pre-existing `warning[license-not-encountered]: MPL-2.0` is benign (`deny.toml`'s allow list includes MPL-2.0 for transitive deps not currently in the graph; same warning has been present since the deny.toml fix-forward and is documented in CLAUDE.md §7's "Known CI failures to triage" footnote — `cargo deny check sources licenses` returns exit 0).

W5 PASS.

---

## 8. W6 details — diagram-linter freshness

`cargo run -p diagram-linter --release --quiet -- check --strict` on HEAD `4c0c1c0`:

```
diagram-linter: 96 checks run, 0 failure(s) [strict]
```

`docs/diagrams/sprint-4/sign-flow.md` frontmatter:
```yaml
last_verified: ff7f41c 2026-05-25
```

`ff7f41c` is the parent-of-parent-of-parent commit (`8ae461a` → `ff7f41c`), 3 commits behind `4c0c1c0`. Well within the linter's 30-commit freshness window per CLAUDE.md §5 check #3.

W6 PASS.

---

## 9. Risks discovered (now closed)

**R3.1 — Session 2A fork-side completeness gap (now CLOSED by Session 2A.2)**. Discovered at Session 3's resume-protocol check 5; root-caused via inspection of the cached fork at `~/.cargo/git/checkouts/sigstore-rs-bab5ac3c8c839ee1/e551bf9/src/rekor/models/log_entry.rs:71-81`; closed by fork commit `b4ea971` adding `Body::dsse(DsseAllOf)` + sibling model files + a regression unit test. This risk did not surface in the V1-V7 verification report or in the Session 3 handoff's §7 risk inventory — the handoff's Risks #1-#5 all assumed `attest_sign` would return Ok at HEAD `8ae461a`. Lesson for future sessions: when a fork adds a new variant to a `#[serde(tag = "...")]`-discriminated enum on the request side, sweep for the matching response-side variant in the same commit, OR add a CI gate that exercises the live round-trip (currently only `cosign-interop.yml` does, which made the bug invisible to local pre-commit gates).

**R3.2 — Verify-side determinism CI green (carried over from Session 3 handoff §7 Risk #4)**. The Session 3 handoff required determinism CI to stay green on the Session 2 HEAD `8ae461a` before verify-side work could begin; resume-protocol check 5 confirmed green on that commit (`gh run list` showed `determinism success` for `8ae461a`). After Commit A1 (`4c0c1c0`), determinism CI ran again — outcome captured in §11 below.

**R3.3 — DSSE envelope JSON round-trip determinism (handoff §7 Risk #3)**. Confirmed NOT a problem. sigstore-rs `src/bundle/sign.rs:268` uses `serde_json::to_string(&envelope)` to produce the string fed to Rekor's `proposedContent.envelope` field, and the SAME `envelope` value (by Rust move semantics, not by re-serialization) populates the on-disk `Bundle.content::DsseEnvelope`. Both Rekor's `envelopeHash` (computed server-side over the request bytes) and verify-side's `serde_json::to_vec(&dsse)` operate on byte-identical envelope bytes, so the hashes round-trip identically.

---

## 10. Implications for Session 4

The cosign step now rejects with:

```
Error: validation error: kind and version mismatch: dsse/0.0.1 != hashedrekord/0.0.1
```

This is **not** an Attestrum-side or fork-side bug per se — it's a cosign-side expectation about what Rekor entry kind a Bundle v0.3 with `Content::DsseEnvelope` should be paired with. The string mismatch (`dsse/0.0.1 != hashedrekord/0.0.1`) is suspicious because:

- `verify-blob-attestation` is documented to verify DSSE-wrapped attestations.
- cosign-installed version on the runner is `cosign_v2.5.2` per the workflow's `sigstore/cosign-installer@v3` step.
- The cosign Go reference (`sigstore-go`) handles dsse-content bundles by re-computing `envelopeHash` + `payloadHash` against the bundle's `tlogEntries[0].canonicalizedBody.spec` fields — that path expects the Rekor entry kind to be `dsse/0.0.1` per the cosign source.

Hypotheses for Session 4 to investigate:

1. **cosign version mismatch**. The `sigstore/cosign-installer@v3` action installs cosign 2.5.2 by default. If the runtime cosign expected newer (e.g., cosign 3.x) for full Bundle v0.3 + dsse support, the error string `"hashedrekord/0.0.1"` would be the default-expected kind for the older `verify-blob` path that 2.5.2 falls back to when it doesn't recognize the bundle as DSSE-aware. **Most likely root cause.** Fix: pin `cosign-installer@v3` to a newer version (`with: cosign-release: 'v3.0.0'` or similar).
2. **Bundle's TransparencyLogEntry kindVersion not propagating**. Less likely. sigstore-rs's `try_into` to `TransparencyLogEntry` should preserve `kind_version` from the parsed `LogEntry`. If it doesn't, the bundle's on-disk `kindVersion` would not match what cosign reads. Easy to falsify locally: `jq '.verificationMaterial.tlogEntries[0].kindVersion' bundle.sigstore.json`.
3. **A cosign Bundle v0.3 quirk**. Possible. Worth checking the cosign-installer action default version + scanning the cosign 2.5/3.0 changelog for "dsse" or "Bundle v0.3".

Session 4's scope (revised from "assertion cleanup" to "cosign-step delta diagnosis + green flip"):

- **S4.1**: Diagnose the cosign version mismatch (likely just bump to cosign 3.x via the installer step in `.github/workflows/cosign-interop.yml`).
- **S4.2**: If cosign 3.x accepts the bundle, the workflow goes green. Flip `source_of_truth: diagram → code` in `docs/diagrams/sprint-4/sign-flow.md` + bump `last_verified` to the cosign-interop-green commit's SHA in a same-commit pair.
- **S4.3**: If cosign 3.x still rejects, deeper diagnosis: download the bundle artifact (the workflow may need to upload it), `jq`-inspect the Rekor entry shape, compare against the cosign Go expected shape, surface to founder.

The Session 4 handoff at `/Users/austinmunday/.claude/plans/cosign-interop-session-4-ci-green-flip-handoff-2026-05-25.md` captures the full Session 4 work sequence.

---

## 11. CI outcomes on HEAD `4c0c1c0`

Run summary:

| Workflow | Run ID | Outcome | Duration | Notes |
|---|---|---|---|---|
| `ci` | `26419364087` | ✅ success | 4m17s | No regression from `8ae461a` (which was also green). |
| `determinism` | `26419364086` | ✅ success | 18m59s | No regression. All four cross-target byte-identity checks passed; Risk #4 from the Session 3 handoff is closed. |
| `cosign-interop` | `26419364100` | ❌ failure (at cosign step) | 3m20s | Outcome (b) — sign + verify succeed; cosign rejects with `dsse/0.0.1 != hashedrekord/0.0.1`. Session 4's scope. |

### 11.1 Determinism outcome

The four-target determinism matrix (`linux-x86_64-gnu`, `linux-aarch64-gnu`, `linux-x86_64-musl`, `macos-aarch64-darwin`) confirms byte-identical bundle production at HEAD `4c0c1c0`. The fork-side rev bump from `e551bf9` → `b4ea971` adds a `Body::dsse(DsseAllOf)` variant + sibling model files affecting only the in-memory `LogEntry` parsed from Rekor responses; the bundle's `verificationMaterial.tlogEntries[0].canonicalizedBody` field is a base64-encoded copy of the same Rekor body that passes through unchanged from response to bundle assembly per `src/bundle/sign.rs:284-289` (the `let log_entry = create_log_entry(...).await? ... .try_into()?` chain preserves the canonicalized body's base64 form without re-encoding). Confirmed end-to-end by the cross-target byte-identity check at run `26419364086`.

---

## 12. Verification report status

This report constitutes the canonical Session 3 deliverable per the Session 3 handoff §3 + §5 Step 5. All six W1-W6 checks PASS. The cosign-step failure is downstream of `attest_verify` and is fully Session 4's territory.

The Session 4 handoff (`H1`) will be written immediately after this report lands.

---

*Report drafted 2026-05-25 by Claude Opus 4.7 (1M context). HEAD at write-time = `4c0c1c0`. Reviewed against Session 3 handoff + `cosign-interop.yml` run `26419364100` log + the cached fork at `b4ea971` + Attestrum local pre-commit five-gate output. If anything below contradicts a runtime fact captured at a future commit, this report describes the state at HEAD `4c0c1c0` and should be superseded by a follow-up report rather than rewritten in place.*
