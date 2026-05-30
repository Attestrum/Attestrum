//! Signed end-to-end chain walk — the crypto layer of the keystone.
//!
//! `#[ignore]`d (requires `SIGSTORE_ID_TOKEN` + network against Sigstore
//! public-good). The default `cargo test` lists but does not run it; the
//! `chain_walk_signed` step in `.github/workflows/cosign-interop.yml` runs it
//! under `--include-ignored` with the GHA-OIDC-exchanged token.
//!
//! Proves the SIGNED path the default-suite `walk_tests` cannot reach:
//! `walk_chain` Sigstore-verifies BOTH the binding and corpus bundles before
//! walking, and rejects a tampered bundle at the crypto layer (`CorpusVerify`).
//! Structural soundness (steps 1–3) is covered offline in `src/lib.rs`.

use std::path::{Path, PathBuf};

use attestrum_attest::{
    DeterminismFields, DigestMap, InTotoStatement, LicensingPosture, ManifestRef, ModelRef,
    RulesetMode, SignalCoverage, Subject, TrainingCorpusPredicate, TRAINING_CORPUS_PREDICATE_TYPE,
};
use attestrum_bind::{
    bind, walk_chain, BindOpts, BindingInput, BoundCorpus, ChainWalkError, ChainWalkOutcome,
    CorpusInput, IdentityPolicy,
};
use attestrum_cas::stream_hash;
use attestrum_core::Modality;
use attestrum_manifest::{
    assign_input_ordinals, assign_occurrence_indices, sort_entries, write_manifest, ManifestEntry,
    ManifestSignals,
};
use attestrum_prove::{ProofKind, ProofTarget};

const EPOCH: i64 = 1_780_012_800; // 2026-05-29T00:00:00Z

fn require_token() -> String {
    match std::env::var("SIGSTORE_ID_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => panic!(
            "chain_walk_signed: SIGSTORE_ID_TOKEN unset/empty — this test must FAIL, not skip, \
             when run. It is #[ignore]'d; CI runs it via --include-ignored where \
             .github/workflows/cosign-interop.yml exports the token from the GHA OIDC exchange."
        ),
    }
}

fn db(b: u8) -> [u8; 32] {
    [b; 32]
}

fn digest_of_file(path: &Path) -> DigestMap {
    let bytes = std::fs::read(path).expect("read file for digest");
    let h = stream_hash(&bytes[..]).expect("stream_hash");
    DigestMap {
        blake3: attestrum_core::hex::encode_32(&h.blake3),
        sha256: attestrum_core::hex::encode_32(&h.sha256),
    }
}

fn sample_entry(doc: u8) -> ManifestEntry {
    ManifestEntry {
        document_id: db(doc),
        sha256: db(doc ^ 0xff),
        size_bytes: u64::from(doc) * 100,
        modality: Modality::Text,
        mime_type: Some("text/plain".into()),
        source_url: Some(format!("file:///docs/doc-{doc:02x}.txt")),
        source_type: None,
        source_dataset_id: None,
        registered_domain: None,
        license_spdx: None,
        language: None,
        fetched_at: None,
        signals: ManifestSignals::default(),
        included: true,
        exclusion_reason: None,
        chunk_refs: None,
        input_ordinal: 0,
        occurrence_index: 0,
    }
}

fn build_manifest(dir: &Path, docs: &[u8]) -> PathBuf {
    let mut entries: Vec<ManifestEntry> = docs.iter().map(|b| sample_entry(*b)).collect();
    assign_input_ordinals(&mut entries);
    sort_entries(&mut entries);
    assign_occurrence_indices(&mut entries);
    let path = dir.join("corpus.parquet");
    write_manifest(&path, &entries).expect("write_manifest");
    path
}

/// Training-corpus Statement whose subject digest is the REAL digest of the
/// manifest file (so sigstore-rs's subject-digest check passes on verify).
fn corpus_statement(manifest: &Path) -> InTotoStatement {
    let entries = attestrum_manifest::read_manifest(manifest).expect("read manifest");
    let leaves: Vec<[u8; 32]> = entries.iter().map(|e| e.document_id).collect();
    let root_hex = attestrum_core::hex::encode_32(&attestrum_merkle::merkle_root(&leaves));
    let byte_count: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let digest_set = digest_of_file(manifest);
    let pred = TrainingCorpusPredicate {
        attestrum_version: "v0.0.1".to_string(),
        builder_version: "attestrum-bind-signed-test/0.0.1".to_string(),
        built_at: "2026-05-29T00:00:00Z".to_string(),
        determinism: DeterminismFields {
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            seed: EPOCH.to_string(),
            manifest_schema_version: "2".to_string(),
        },
        manifest: ManifestRef {
            uri: format!("file://{}", manifest.display()),
            digest_set: digest_set.clone(),
            row_count: entries.len() as u64,
            byte_count,
        },
        merkle_root: root_hex,
        merkle_algorithm: "blake3-rfc6962".to_string(),
        ruleset_mode: RulesetMode::Strict,
        ruleset_id: "attestrum-default".to_string(),
        ruleset_version: "v0.1.0".to_string(),
        signal_coverage: SignalCoverage::default(),
        licensing_posture: LicensingPosture::AllOpenLicensed,
        license_inventory: vec![],
        takedown_contact: None,
        dataset_homepage: None,
        publication_intent: None,
        total_compute: None,
        training_cost: None,
        model_name: None,
    };
    let subject = Subject {
        name: "manifest.parquet".to_string(),
        digest: digest_set,
    };
    InTotoStatement::new(
        TRAINING_CORPUS_PREDICATE_TYPE,
        vec![subject],
        serde_json::to_value(&pred).expect("predicate to value"),
    )
}

/// Sign a Statement against Sigstore public-good, return the bundle path.
fn sign_statement(dir: &Path, name: &str, stmt: &InTotoStatement, token: &str) -> PathBuf {
    let canonical = stmt.canonical_json().expect("canonical_json");
    let out = dir.join(format!("{name}.sigstore.json"));
    let signed = attestrum_attest::sign(attestrum_attest::SignRequest {
        statement_payload: canonical.as_bytes(),
        bundle_output_path: &out,
        oidc_id_token: token.to_string(),
    })
    .expect("sign against public-good");
    signed.bundle_path
}

#[test]
#[ignore = "requires SIGSTORE_ID_TOKEN + network; runs in .github/workflows/cosign-interop.yml only"]
fn chain_walk_signed() {
    let token = require_token();
    let dir = std::env::temp_dir().join(format!("attestrum-bind-signed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create tmpdir");

    // --- Corpus: real manifest + matching-subject-digest statement, signed. ---
    let manifest = build_manifest(&dir, &[1, 2, 3]);
    let corpus_stmt = corpus_statement(&manifest);
    let corpus_bundle = sign_statement(&dir, "corpus", &corpus_stmt, &token);

    // --- Model: a weights-manifest file whose real digest IS the model digest. ---
    let model_manifest = dir.join("model.weights.json");
    std::fs::write(&model_manifest, br#"{"weights":"acme-model-m"}"#)
        .expect("write model manifest");
    let model_digest = digest_of_file(&model_manifest);
    let model = ModelRef {
        identity: "https://huggingface.co/acme/model-m".to_string(),
        weights_manifest_digest: model_digest.clone(),
        signing_bundle_ref: None,
    };

    // --- Binding: bind the model to the SIGNED corpus bundle, then sign it. ---
    let binding_artifact = bind(&BindOpts {
        model_card_uri: model.identity.clone(),
        model,
        corpora: vec![BoundCorpus {
            bundle_path: corpus_bundle.clone(),
            role: "pretraining".to_string(),
        }],
        builder_identity: "Acme AI".to_string(),
        config_digest: None,
        source_date_epoch: EPOCH,
        builder_version: "attestrum-bind-signed-test/0.0.1".to_string(),
        sign: true,
        oidc_id_token: Some(token.clone()),
        workspace: Some(dir.join("ws")),
    })
    .expect("bind + sign");
    let binding_bundle = binding_artifact
        .bundle_path
        .expect("signed binding has a bundle");

    // Identity policy: pluck the real SAN/issuer from the signed binding bundle
    // via verify_statement (verify() pins training-corpus and so can't read a
    // model-binding bundle's identity).
    let v = attestrum_attest::verify_statement(attestrum_attest::VerifyRequest {
        bundle_path: &binding_bundle,
        manifest_path: &model_manifest,
        identity_regex: ".*",
        issuer_regex: ".*",
        offline: false,
        expected_predicate_type: Some(attestrum_attest::MODEL_BINDING_PREDICATE_TYPE),
    })
    .expect("binding self-verify");
    let identity_pattern = format!("^{}$", regex::escape(&v.identity));
    let issuer_pattern = format!("^{}$", regex::escape(&v.oidc_issuer));

    let policy = IdentityPolicy {
        identity_regex: &identity_pattern,
        issuer_regex: &issuer_pattern,
        offline: false,
    };

    // --- Positive: the full signed chain walks to an inclusion. ---
    let outcome = walk_chain(
        &model_digest,
        BindingInput {
            bundle_path: &binding_bundle,
            manifest_path: &model_manifest,
            policy,
        },
        CorpusInput {
            bundle_path: &corpus_bundle,
            manifest_path: &manifest,
            policy,
        },
        ProofTarget::Blake3(db(2)),
    )
    .expect("signed walk to inclusion");
    match outcome {
        ChainWalkOutcome::InCorpus { role, proof } => {
            assert_eq!(role, "pretraining");
            assert_eq!(proof.kind, ProofKind::Inclusion);
        }
        other => panic!("expected InCorpus, got {other:?}"),
    }

    // --- Negative: a flipped corpus DSSE signature is rejected at CorpusVerify
    // (the binding still verifies — proving the rejection is the CORPUS bundle's
    // crypto, not a structural step). ---
    let tampered_corpus = dir.join("corpus.flipped.sigstore.json");
    {
        let mut bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&corpus_bundle).expect("read corpus bundle"))
                .expect("parse corpus bundle");
        let sig_b64 = bundle["dsseEnvelope"]["signatures"][0]["sig"]
            .as_str()
            .expect("corpus sig present")
            .to_string();
        let mut raw = base64_decode(&sig_b64);
        raw[0] ^= 0x01; // flip one bit
        bundle["dsseEnvelope"]["signatures"][0]["sig"] =
            serde_json::Value::String(base64_encode(&raw));
        std::fs::write(&tampered_corpus, serde_json::to_vec(&bundle).unwrap())
            .expect("write tampered corpus");
    }
    let err = walk_chain(
        &model_digest,
        BindingInput {
            bundle_path: &binding_bundle,
            manifest_path: &model_manifest,
            policy,
        },
        CorpusInput {
            bundle_path: &tampered_corpus,
            manifest_path: &manifest,
            policy,
        },
        ProofTarget::Blake3(db(2)),
    )
    .expect_err("flipped corpus signature must be rejected");
    assert!(
        matches!(err, ChainWalkError::CorpusVerify(_)),
        "expected CorpusVerify for a flipped corpus signature, got {err:?}"
    );
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .expect("base64 decode")
}

fn base64_encode(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}
