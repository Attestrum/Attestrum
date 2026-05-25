#!/usr/bin/env python3
"""Hand-crafted asciinema v2 cast for the Sprint 3 acceptance demo.

Captures the verbatim stdout of `cargo run --release --quiet -p attestrum-cli
--example sprint-3-demo` as observed on the development machine, and
emits a properly-JSON-escaped asciinema v2 cast at
`docs/demos/sprint-3.cast`.

Why a Python generator instead of asciinema recording? Same rationale
as Sprint 1 E12 and Sprint 2 E10 — JSON escaping is a footgun,
asciinema recording adds wall-clock noise, and a hand-crafted cast
plays smoothly every time.

Re-run this script if the demo body changes:

    python3 tools/cast/sprint-3.py > docs/demos/sprint-3.cast

The CAPTURE list below is the canonical reference; update it whenever
the demo's printed output changes. Per-process pid + nanos in temp
paths are substituted with the literal `<pid>-<nanos>` placeholder
so the cast doesn't pin a specific PID/nanos that no future
invocation will reproduce. Merkle root hex values + leaf counts +
total bytes + per-shard distribution ARE pinned because the corpus
content is deterministic.

Asciinema v2 format: https://docs.asciinema.org/manual/asciicast/v2/
"""
import json
import sys

# Header: width/height match Sprint 1 + Sprint 2 casts for visual
# consistency. Timestamp pinned to 2026-05-24 17:00:00 UTC (Sprint 3
# close).
HEADER = {
    "version": 2,
    "width": 100,
    "height": 50,
    "timestamp": 1748113200,
    "env": {"SHELL": "/bin/zsh", "TERM": "xterm-256color"},
    "title": "Attestrum Sprint 3 acceptance demo",
}

# The command the user types at the prompt.
COMMAND = "cargo run --release --quiet -p attestrum-cli --example sprint-3-demo"

# Verbatim stdout from the demo binary as captured on the dev machine
# (macOS aarch64, 2026-05-24). Per-process pid + nanos in paths
# replaced with `<pid>-<nanos>` placeholder text. Merkle roots + leaf
# counts + per-shard distribution are deterministic functions of the
# corpus content and shard count, so they're pinned exactly.
CAPTURE = [
    "=== Attestrum Sprint 3 acceptance demo ===\r\n",
    "\r\n",
    "--- 100-document synthetic corpus ---\r\n",
    "  wrote 100 input files at /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/inputs/\r\n",
    "  wrote corpus.toml at /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/corpus.toml\r\n",
    "\r\n",
    "--- E4 + E5: attestrum build (unsharded) ---\r\n",
    "attestrum build: ok\r\n",
    "  merkle_root:  9fa25518ae09d2b8ae8134e14912a793ff405a1a658c8478b752fc73f582c501\r\n",
    "  manifest:     /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/ws-unsharded/.attestrum/manifests/manifest.parquet\r\n",
    "  leaf_count:   100\r\n",
    "  total_bytes:  2800\r\n",
    "\r\n",
    "--- E6: attestrum inspect (unsharded) ---\r\n",
    "merkle_root: 9fa25518ae09d2b8ae8134e14912a793ff405a1a658c8478b752fc73f582c501\r\n",
    "leaf_count:  100\r\n",
    "total_bytes: 2800\r\n",
    "per modality:\r\n",
    "  text: 100\r\n",
    "\r\n",
    "--- E7: attestrum plan --shards 4 ---\r\n",
    "attestrum plan: ok\r\n",
    "  shards_requested: 4\r\n",
    "  shards_emitted:   4\r\n",
    "  entries:          100\r\n",
    "  out:              /tmp/attestrum-sprint-3-demo-<pid>-<nanos>\r\n",
    "\r\n",
    "--- E7: attestrum build (per-shard) ---\r\n",
    "  building shard-0000 -> ws-shard-0000/\r\n",
    "attestrum build: ok\r\n",
    "  merkle_root:  a296bff555112455e4b9e9b58d95d3445f68747e0320a01d479b7a9b79d7472a\r\n",
    "  manifest:     /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/ws-shard-0000/.attestrum/manifests/manifest.parquet\r\n",
    "  leaf_count:   32\r\n",
    "  total_bytes:  896\r\n",
    "  building shard-0001 -> ws-shard-0001/\r\n",
    "attestrum build: ok\r\n",
    "  merkle_root:  3b67bb72867ed45f205c572ff6203b7cdcea26402177b2eb712e232d7f833fcd\r\n",
    "  manifest:     /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/ws-shard-0001/.attestrum/manifests/manifest.parquet\r\n",
    "  leaf_count:   18\r\n",
    "  total_bytes:  504\r\n",
    "  building shard-0002 -> ws-shard-0002/\r\n",
    "attestrum build: ok\r\n",
    "  merkle_root:  e750f774e3dcb417271140fbbf2ec7bcc70a9afc5e7de9466f179775e7c2289b\r\n",
    "  manifest:     /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/ws-shard-0002/.attestrum/manifests/manifest.parquet\r\n",
    "  leaf_count:   23\r\n",
    "  total_bytes:  644\r\n",
    "  building shard-0003 -> ws-shard-0003/\r\n",
    "attestrum build: ok\r\n",
    "  merkle_root:  b1eb289b6e75990ad80c96db79364711d1f6a23764dd186535ba0e5071ac1b4d\r\n",
    "  manifest:     /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/ws-shard-0003/.attestrum/manifests/manifest.parquet\r\n",
    "  leaf_count:   27\r\n",
    "  total_bytes:  756\r\n",
    "\r\n",
    "--- E7: attestrum merge ---\r\n",
    "attestrum merge: ok\r\n",
    "  inputs: 4\r\n",
    "  rows:   100\r\n",
    "  out:    /tmp/attestrum-sprint-3-demo-<pid>-<nanos>/merged.parquet\r\n",
    "\r\n",
    "--- E6: attestrum inspect (merged) ---\r\n",
    "merkle_root: 9fa25518ae09d2b8ae8134e14912a793ff405a1a658c8478b752fc73f582c501\r\n",
    "leaf_count:  100\r\n",
    "total_bytes: 2800\r\n",
    "per modality:\r\n",
    "  text: 100\r\n",
    "\r\n",
    "--- merge round-trip check ---\r\n",
    "  unsharded root: 9fa25518ae09d2b8ae8134e14912a793ff405a1a658c8478b752fc73f582c501\r\n",
    "  merged    root: 9fa25518ae09d2b8ae8134e14912a793ff405a1a658c8478b752fc73f582c501\r\n",
    "  -> MATCH\r\n",
    "\r\n",
    "=== SPRINT 3 COMPLETE ===\r\n",
    "    manifest writer + Rayon pipeline + attestrum build/inspect/plan/merge = green\r\n",
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
    # The demo has ~75 lines of output; at 0.14s per line that plays
    # in about 11s — fast enough to be lively, slow enough to follow.
    t = round(t + 0.4, 2)
    line_dt = 0.14
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
