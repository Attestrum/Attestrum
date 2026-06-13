//! The two-`prove()` removal flow.
//!
//! [`build_removal`] proves the target included in `--before` and absent from
//! `--after`, validates that each proof is genuinely the kind a removal
//! requires, and assembles a [`crate::report::RemovalReport`]. Both `prove()`
//! calls run with `sign=false`: the report is unsigned read-only evidence.

use crate::report::{RemovalReport, SideProof};
use attestrum_prove::{
    prove, AttestrumProveError, ManifestSource, ProofKind, ProofTarget, ProveOpts,
};
use std::path::Path;
use thiserror::Error;

/// Failure modes of [`build_removal`].
#[derive(Debug, Error)]
pub enum RemoveError {
    #[error("proving inclusion against --before failed")]
    Before(#[source] AttestrumProveError),

    #[error("proving non-inclusion against --after failed")]
    After(#[source] AttestrumProveError),

    #[error(
        "target {0} is not present in --before; a removal proof needs the document to exist in the \
         earlier version"
    )]
    TargetNotInBefore(String),

    #[error("target {0} is still present in --after; it was not removed between the two versions")]
    TargetStillInAfter(String),
}

/// Prove that `target` was in `before` and is absent from `after`, returning the
/// bundled removal report. `source_date_epoch` flows into each proof's
/// reproducible `built_at`, so the same inputs + epoch yield byte-identical
/// reports per manifest path.
pub fn build_removal(
    target: [u8; 32],
    before: &Path,
    after: &Path,
    source_date_epoch: i64,
) -> Result<RemovalReport, RemoveError> {
    let opts = ProveOpts {
        sign: false,
        source_date_epoch,
        oidc_id_token: None,
        workspace: None,
        corpus_bundle_path: None,
        cas_root: None,
        no_index: false,
    };

    let target_hex = hex_64(&target);

    // 1. Inclusion against --before: the document must have been there.
    let inclusion = prove(
        ProofTarget::Blake3(target),
        ManifestSource::Local(before.to_path_buf()),
        &opts,
    )
    .map_err(RemoveError::Before)?;
    if inclusion.kind != ProofKind::Inclusion {
        return Err(RemoveError::TargetNotInBefore(target_hex));
    }

    // 2. Non-inclusion against --after: the document must be gone.
    let non_inclusion = prove(
        ProofTarget::Blake3(target),
        ManifestSource::Local(after.to_path_buf()),
        &opts,
    )
    .map_err(RemoveError::After)?;
    if non_inclusion.kind != ProofKind::NonInclusion {
        return Err(RemoveError::TargetStillInAfter(target_hex));
    }

    Ok(RemovalReport::new(
        target_hex,
        SideProof::inclusion(before.display().to_string(), inclusion.statement),
        SideProof::non_inclusion(after.display().to_string(), non_inclusion.statement),
    ))
}

/// Lowercase hex of a 32-byte digest.
pub fn hex_64(b: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(64);
    for byte in b {
        let _ = write!(s, "{byte:02x}");
    }
    s
}
