//! Shared OIDC id_token resolution for the keyless-signing subcommands
//! (`sign`, `bind`, `prove`). Extracted from the previously-duplicated bodies
//! in `sign.rs` and `bind.rs` per CLAUDE.md §14 — `prove` is the third
//! use-site, so the resolution logic now lives here once.
//!
//! Resolution order: `--oidc-token-file <PATH>` (read + trimmed; errors if
//! empty) takes precedence over the `SIGSTORE_ID_TOKEN` env var. The returned
//! `Err` is the bare message; each caller prefixes `attestrum <cmd>: ` and
//! maps its own exit code.

use std::path::Path;

/// Resolve the Sigstore OIDC id_token (raw JWT) used for keyless signing.
///
/// `unsigned_available` toggles whether the "no token" error suggests
/// `--unsigned` — `true` for callers that can skip signing (`bind`, `prove`),
/// `false` for `sign`, which always signs (it only has `--offline`).
pub(crate) fn resolve_oidc_token(
    oidc_token_file: Option<&Path>,
    unsigned_available: bool,
) -> Result<String, String> {
    if let Some(path) = oidc_token_file {
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
        _ => Err(missing_token_msg(unsigned_available)),
    }
}

/// The "no token found" error message, split out so it can be unit-tested
/// without mutating the process-global `SIGSTORE_ID_TOKEN` env var.
fn missing_token_msg(unsigned_available: bool) -> String {
    let mut msg = String::from(
        "OIDC id_token required to sign: pass --oidc-token-file <PATH> or set \
         SIGSTORE_ID_TOKEN env var",
    );
    if unsigned_available {
        msg.push_str(", or pass --unsigned");
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("attestrum-oidc-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn file_token_is_trimmed() {
        let p = scratch("trim");
        fs::write(&p, "  tok-abc \n").unwrap();
        let got = resolve_oidc_token(Some(&p), false).unwrap();
        assert_eq!(got, "tok-abc");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn empty_file_after_trim_errors() {
        let p = scratch("empty");
        fs::write(&p, "   \n\t ").unwrap();
        let err = resolve_oidc_token(Some(&p), false).unwrap_err();
        assert!(err.contains("empty after trim"), "{err}");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn unreadable_file_errors() {
        let p = scratch("missing-does-not-exist");
        let _ = fs::remove_file(&p);
        let err = resolve_oidc_token(Some(&p), false).unwrap_err();
        assert!(err.contains("failed to read --oidc-token-file"), "{err}");
    }

    #[test]
    fn missing_token_message_hint_toggles() {
        assert!(!missing_token_msg(false).contains("--unsigned"));
        assert!(missing_token_msg(true).contains("--unsigned"));
        assert!(missing_token_msg(false).contains("SIGSTORE_ID_TOKEN"));
    }
}
