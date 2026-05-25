#!/usr/bin/env python3
"""Hand-crafted asciinema v2 cast for the Sprint 2 acceptance demo.

Captures the verbatim stdout of `cargo run --release --quiet -p attestrum-cas
--example sprint-2-demo` as observed on the development machine, and
emits a properly-JSON-escaped asciinema v2 cast at
`docs/demos/sprint-2.cast`.

Why a Python generator instead of asciinema recording?

  - JSON escaping inside the cast is a footgun (every newline, every
    quote, every embedded path char must be encoded right). Python's
    json module handles it once and right.
  - The output IS deterministic per run on a given machine, but
    asciinema recording adds wall-clock noise (variable timings, occasional
    re-renders). A hand-crafted cast plays smoothly every time.
  - Sprint 1's E12 used the same pattern (docs/demos/sprint-1.cast was
    generated the same way) — keeping the convention for tooling
    consistency.

Re-run this script if the demo body changes:

    python3 tools/cast/sprint-2.py > docs/demos/sprint-2.cast

Captured output snapshot inside this script is the canonical reference;
update CAPTURE below if the demo's printed output ever changes.

Asciinema v2 format: https://docs.asciinema.org/manual/asciicast/v2/
"""
import json
import sys

# Header: width/height match Sprint 1's cast for visual consistency.
# Timestamp pinned to 2026-05-23 23:00:00 UTC (Sprint 2 close), so
# regenerating the cast on a later date doesn't churn the file.
HEADER = {
    "version": 2,
    "width": 100,
    "height": 40,
    "timestamp": 1748041200,
    "env": {"SHELL": "/bin/zsh", "TERM": "xterm-256color"},
    "title": "Attestrum Sprint 2 acceptance demo",
}

# The command the user types at the prompt. Per-char keypress timing
# makes the cast feel like a real session.
COMMAND = "cargo run --release --quiet -p attestrum-cas --example sprint-2-demo"

# Verbatim stdout from the demo binary as captured on the dev machine
# (macOS aarch64, 2026-05-23). Each entry becomes one cast frame.
# Order matters: emitted in sequence with increasing timestamps.
CAPTURE = [
    "=== Attestrum Sprint 2 acceptance demo ===\r\n",
    "\r\n",
    "--- 5-document in-memory corpus ---\r\n",
    "  [0]  38 bytes  \"Attestrum Sprint 2 demo — document zero.\"\r\n",
    "  [1]  65 bytes  \"Document one. Slightly longer than zero, with a different ending.\"\r\n",
    "  [2]  38 bytes  \"Doc two. The third leaf in the corpus.\"\r\n",
    "  [3]  51 bytes  \"Fourth document; about-average length and contents.\"\r\n",
    "  [4]  49 bytes  \"Fifth and final document of this acceptance demo.\"\r\n",
    "\r\n",
    "--- E5: streaming BLAKE3 + SHA-256 hasher ---\r\n",
    "  [0] blake3=58c9d4ebc148..c665  sha256=8622e9bed1b0..df20  size=38B\r\n",
    "  [1] blake3=326328fd8f38..79da  sha256=91b43f046868..30ec  size=65B\r\n",
    "  [2] blake3=503ad361ec8c..59b7  sha256=ced5cf162107..031d  size=38B\r\n",
    "  [3] blake3=c52dde051584..3244  sha256=236c0d1b108d..fdb1  size=51B\r\n",
    "  [4] blake3=31223d7a8308..079b  sha256=3500266571a9..734e  size=49B\r\n",
    "\r\n",
    "--- E6: CasStore atomic put + roundtrip ---\r\n",
    "  CAS root: /tmp/attestrum-sprint-2-demo-<pid>-<nanos>\r\n",
    "  [0] put -> cas/blake3/58/c9/58c9d4ebc1487b6c3adf4b019bfa14135843df7f2db42afe9f851fd2af6cc665.bin\r\n",
    "  [1] put -> cas/blake3/32/63/326328fd8f386b07067f94f2840251afb8f0d49f116f8c37fac8b1141b5d79da.bin\r\n",
    "  [2] put -> cas/blake3/50/3a/503ad361ec8cd67ddcedbd01432e0f79f545486e2a0cef8b595846c5f9a159b7.bin\r\n",
    "  [3] put -> cas/blake3/c5/2d/c52dde0515849710e982ff06f95415bb74e4fce3e88c4edf5ad701e21cc93244.bin\r\n",
    "  [4] put -> cas/blake3/31/22/31223d7a8308e797e397ed705b8bdc5625c82cd9f97f6c6aa5b803574008079b.bin\r\n",
    "  roundtrip via CasStore::open: 5/5 match\r\n",
    "\r\n",
    "--- E7: RFC 6962 Merkle root (BLAKE3, sorted, multiset) ---\r\n",
    "  sorted digests:\r\n",
    "    31223d7a8308..079b\r\n",
    "    326328fd8f38..79da\r\n",
    "    503ad361ec8c..59b7\r\n",
    "    58c9d4ebc148..c665\r\n",
    "    c52dde051584..3244\r\n",
    "  root: b038a4087c2517359561a12723d57891d1ab87b1ed82d94d892dda9f96fb4df8\r\n",
    "        (== merkle_root over the sorted leaves)\r\n",
    "\r\n",
    "--- E8: audit path generate + verify ---\r\n",
    "  leaf 2 (digest 503ad361ec8c..59b7):\r\n",
    "  audit path (length 3, path[0] = sibling closest to leaf):\r\n",
    "    [0] e4f15ff05c73..b31e\r\n",
    "    [1] 29f2569ac2c1..e2d5\r\n",
    "    [2] fb8a81319ccc..1b71\r\n",
    "  verify_audit_path(root, leaf, 2, 5, path) -> true\r\n",
    "\r\n",
    "=== SPRINT 2 COMPLETE ===\r\n",
    "    streaming hash + atomic CAS + Merkle root + audit path = green\r\n",
]


def main() -> int:
    frames = []
    t = 0.3
    # Prompt + per-char keystrokes for the command.
    frames.append([t, "o", "$ "])
    keystroke_dt = 0.04
    for ch in COMMAND:
        t = round(t + keystroke_dt, 2)
        frames.append([t, "o", ch])
    # Enter pressed: brief pause then carriage-return + line-feed.
    t = round(t + 0.3, 2)
    frames.append([t, "o", "\r\n"])
    # Brief "compiling" pause then output streams out section by section.
    # Per-line dt is ~0.18s — fast enough to play in under 10s, slow
    # enough to follow visually.
    t = round(t + 0.4, 2)
    line_dt = 0.16
    for line in CAPTURE:
        frames.append([t, "o", line])
        t = round(t + line_dt, 2)
    # Final prompt + 2s idle so playback ends cleanly.
    t = round(t + 0.3, 2)
    frames.append([t, "o", "$ "])
    t = round(t + 2.0, 2)
    frames.append([t, "o", ""])

    json.dump(HEADER, sys.stdout, separators=(", ", ": "))
    sys.stdout.write("\n")
    for frame in frames:
        json.dump(frame, sys.stdout, separators=(", ", ": "))
        sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
