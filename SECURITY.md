# Security Policy

## Reporting a vulnerability

Email **security@attestrum.com** with the details. Please include:

- A description of the vulnerability and the affected component or version.
- Reproduction steps or a proof-of-concept where possible.
- Your suggested remediation, if any.
- Whether you would like credit in the eventual advisory.

You will receive an acknowledgement within 48 hours. We aim to land a fix in `main` within 14 days of confirmation; complex issues that require a coordinated upstream fix (sigstore-rs, in-toto specs) may take longer and we will keep you informed.

Please do **not** disclose publicly until we have published an advisory or the 90-day disclosure window has elapsed, whichever comes first.

## What counts as a security issue

Anything that breaks the cryptographic correctness of an Attestrum bundle is a security issue, even if it doesn't fit the traditional shape of a CVE:

- Determinism bugs (the same input producing different bundles on different platforms).
- Merkle-root, BLAKE3, or SHA-256 implementation bugs that admit collisions or wrong-output.
- Signing or verification path bugs that admit forged bundles or accept tampered bundles.
- Bundle assembly bugs that produce non-spec-conformant Sigstore Bundle v0.3 or in-toto Statement v1 output.
- CAS layout or manifest schema regressions that silently corrupt corpus-identity invariants.
- Any path that bypasses the protected-systems policy described in `CHANGELOG.md`.

Regular bugs that don't compromise correctness should be filed as GitHub issues, not security reports.
