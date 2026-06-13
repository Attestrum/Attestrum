//! Report assembly and serialization.
//!
//! The canonical `report.json` is produced through
//! [`attestrum_attest::deterministic_json`] — the workspace's single sanctioned
//! sort-then-serialize primitive. The report embeds the two in-toto Statements
//! verbatim, so anyone can re-verify the evidence without trusting the summary.
//! Because each proof embeds a `file://` manifest URI, the bytes are
//! deterministic *per manifest path* (the determinism test pins byte-identity
//! across repeated runs on the same inputs).

use attestrum_prove::InTotoStatement;
use serde::Serialize;
use std::fmt::Write as _;

#[derive(Debug, Serialize)]
pub struct RemovalReport {
    pub tool: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Lowercase-hex BLAKE3 `document_id` proved removed.
    pub target: String,
    /// `true` once both directions validate (inclusion-before + non-inclusion-after).
    pub removed: bool,
    pub before: SideProof,
    pub after: SideProof,
}

#[derive(Debug, Serialize)]
pub struct SideProof {
    /// Manifest path the proof was generated against.
    pub manifest: String,
    /// `"inclusion"` or `"non-inclusion"`.
    pub proof_kind: String,
    /// The in-toto v1 Statement wrapping the proof predicate.
    pub statement: InTotoStatement,
}

impl SideProof {
    pub fn inclusion(manifest: String, statement: InTotoStatement) -> Self {
        Self {
            manifest,
            proof_kind: "inclusion".to_string(),
            statement,
        }
    }

    pub fn non_inclusion(manifest: String, statement: InTotoStatement) -> Self {
        Self {
            manifest,
            proof_kind: "non-inclusion".to_string(),
            statement,
        }
    }
}

impl RemovalReport {
    pub fn new(target: String, before: SideProof, after: SideProof) -> Self {
        Self {
            tool: "attestrum-remove".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: None,
            target,
            removed: true,
            before,
            after,
        }
    }

    /// Serialize to canonical JSON (recursive key sort, compact) with a single
    /// trailing newline.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut s = attestrum_attest::deterministic_json(self)?;
        s.push('\n');
        Ok(s)
    }

    /// Render the human-readable Markdown summary.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        let _ = write!(
            md,
            "# attestrum remove — removal evidence (v{})\n\n",
            self.version
        );
        if let Some(ts) = &self.timestamp {
            let _ = writeln!(md, "- generated: {ts}");
        }
        let _ = writeln!(md, "- target: `{}`", self.target);
        let _ = writeln!(
            md,
            "- result: **{}**",
            if self.removed {
                "removed"
            } else {
                "not removed"
            }
        );
        let _ = writeln!(md, "\n## Evidence\n");
        let _ = writeln!(
            md,
            "1. **{}** against `{}` — proves the document was present.",
            self.before.proof_kind, self.before.manifest
        );
        let _ = writeln!(
            md,
            "2. **{}** against `{}` — proves the document is absent.",
            self.after.proof_kind, self.after.manifest
        );
        let _ = writeln!(
            md,
            "\nBoth in-toto Statements are embedded in `report.json`; verify them with stock `cosign` — no \
             Attestrum install required."
        );
        md
    }
}
