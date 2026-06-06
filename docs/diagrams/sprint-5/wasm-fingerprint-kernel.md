---
title: "C2 wasm text-MinHash kernel — one source, native + wasm consumers, byte-identity gates"
models: "crates/attestrum-fingerprint-wasm/src/lib.rs, crates/attestrum-text-minhash/src/lib.rs, crates/attestrum-text-minhash/src/minhash.rs, crates/attestrum-fingerprint-wasm/tests/crosscheck.rs, MINHASH_PERMS, normalize_text, fingerprint_text"
source_of_truth: code
last_verified: 9681203 2026-06-06
diagram_type: flowchart
---

# C2 — wasm text-MinHash kernel

Source of truth: **`code`**. `crates/attestrum-fingerprint-wasm/src/lib.rs` and the
`attestrum-text-minhash` kernel it wraps are authoritative; this diagram is the derived view.

**One kernel, two compile targets.** The PROTECTED text-MinHash kernel (CLAUDE.md §4) lives in
`attestrum-text-minhash` (`normalize_text` + `minhash::compute`, extracted byte-identically in C1).
Two crates consume it: `attestrum-fingerprint` (native, via the public `fingerprint_text` path) and the
new `attestrum-fingerprint-wasm` (a `cdylib` compiled to `wasm32-unknown-unknown`). The wasm crate adds
**no algorithm** — it only marshals bytes across a raw `extern "C"` ABI (`attestrum_alloc` →
`attestrum_minhash` → `attestrum_dealloc`, writing `MINHASH_PERMS` = 128 little-endian `u64`). Because
the browser runs the *identical Rust*, there is no second implementation that could drift from the CLI.

**The §4 safeguard is the cross-check gate, not a footer.** C2 is founder-confirmed routine (the C1 §4
footer already named "byte-identical WASM reuse" as its approved purpose, and C2 changes no protected
parameter). What actually prevents drift is byte-identity enforced in CI (`.github/workflows/wasm.yml`):

- **reproducible build** — each host (x86 + arm) builds the `wasm-release` artifact twice and `cmp`s
  same-host, proving the committed/served `.wasm` re-verifies on its build toolchain. (We deliberately
  do *not* `cmp` the binary across host arches: rustc→wasm32 codegen is reproducible per host but not
  bit-identical between x86 and arm hosts, and the artifact is built once — so cross-arch *binary*
  identity is neither required nor achievable. The host-independent property that matters is the
  *output*, proven next, on both arches.)
- **output cross-check** (load-bearing) — a no-dependency Node loader (`tools/wasm-crosscheck/run.mjs`)
  runs every passage through the **actual** `.wasm` and diffs against the committed golden
  (`crates/attestrum-fingerprint-wasm/tests/golden/minhash-vectors.txt`), on **both** x86 and arm. A
  native-only golden can't catch wasm-codegen / pure-blake3 drift; running the real artifact can.

The golden is produced from the **native** kernel (`examples/gen_golden.rs`); `tests/crosscheck.rs`
ties the native kernel, the `extern "C"` export, and `fingerprint_text`'s public output all to that same
golden. So: native kernel ≡ extern export ≡ public API (native test) and real wasm ≡ golden (Node gate)
→ the browser's near-match answer is byte-identical to the CLI's.

`pure` blake3 is scoped to the wasm32 target only, so native `cargo test --workspace` keeps the default
backend; the P1 spike proved `pure` == default byte-for-byte.

```mermaid
flowchart TB
  classDef protected fill:#7a1f1f,stroke:#c63737,color:#fff
  classDef native fill:#1f6f3f,stroke:#3ec072,color:#fff
  classDef wasm fill:#1a3a6f,stroke:#3a8ed7,color:#fff
  classDef gate fill:#8a5a00,stroke:#e0a52e,color:#fff
  classDef future fill:#3a3a3a,stroke:#666,color:#aaa

  subgraph kernel["attestrum-text-minhash (PROTECTED §4)"]
    NT["normalize_text<br/>NFC · lowercase · ws-collapse"]
    MC["minhash::compute<br/>128 perms · 5-gram · BLAKE3-keyed"]
    NT --> MC
  end
  class NT,MC protected

  subgraph nativec["native consumer"]
    FT["attestrum-fingerprint<br/>fingerprint_text (public API)"]
  end
  class FT native

  subgraph wasmc["attestrum-fingerprint-wasm (cdylib)"]
    AL["attestrum_alloc"]
    MH["attestrum_minhash<br/>(writes MINHASH_PERMS LE u64)"]
    DA["attestrum_dealloc"]
    AL --> MH --> DA
  end
  class AL,MH,DA wasm

  WASMART["attestrum_fingerprint_wasm.wasm<br/>(--profile wasm-release)"]
  class WASMART wasm

  GOLD["tests/golden/minhash-vectors.txt<br/>(from examples/gen_golden.rs, native)"]
  class GOLD native

  G1["CI: reproducible build<br/>cmp same-host (x86 + arm)"]
  G2["CI: output cross-check<br/>run.mjs: real wasm == golden (x86 + arm)"]
  XT["tests/crosscheck.rs<br/>kernel ≡ extern ≡ fingerprint_text"]
  class G1,G2,XT gate

  BROWSER["attestrum.com near-match demo<br/>(C4 — browser glue, deferred)"]
  class BROWSER future

  MC --> FT
  MC --> MH
  MH --> WASMART
  FT --> GOLD
  MC --> XT
  MH --> XT
  WASMART --> G1
  WASMART --> G2
  GOLD --> G2
  GOLD --> XT
  WASMART -.-> BROWSER
```
