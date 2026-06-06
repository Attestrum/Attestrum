// wasm cross-check loader — the load-bearing C2 gate.
//
// Instantiates the ACTUAL attestrum-fingerprint-wasm `.wasm`, runs every golden
// passage through it via the same alloc/call/read ABI the browser glue uses,
// and asserts each 128-permutation MinHash signature equals the committed
// golden (tests/golden/minhash-vectors.txt). A native Rust golden test cannot
// substitute: only running the real artifact proves the wasm codegen + the
// pure-blake3 backend produce byte-identical output to the native kernel. This
// loader is also the reference alloc->call->read pattern C4's browser glue reuses.
//
// Usage:
//   node tools/wasm-crosscheck/run.mjs <path-to.wasm> [path-to-golden.txt]
//
// Exit 0 = every passage matches; exit 1 = any mismatch (or bad args). No
// dependencies — Node's built-in WebAssembly + fs only.

import { readFileSync } from "node:fs";

const MINHASH_PERMS = 128;
const SIG_BYTES = MINHASH_PERMS * 8;

const wasmPath = process.argv[2];
const goldenPath =
  process.argv[3] ??
  new URL(
    "../../crates/attestrum-fingerprint-wasm/tests/golden/minhash-vectors.txt",
    import.meta.url,
  );

if (!wasmPath) {
  console.error("usage: node run.mjs <path-to.wasm> [golden.txt]");
  process.exit(1);
}

function parseGolden(text) {
  const rows = [];
  for (const line of text.split("\n")) {
    if (line.length === 0 || line.startsWith("#")) continue;
    const cols = line.split("\t");
    if (cols.length !== 3) {
      console.error(`malformed golden line (need 3 tab cols): ${line}`);
      process.exit(1);
    }
    const [label, input, hexes] = cols;
    const expected = hexes.split(",");
    if (expected.length !== MINHASH_PERMS) {
      console.error(`golden line ${label} has ${expected.length} perms, want ${MINHASH_PERMS}`);
      process.exit(1);
    }
    rows.push({ label, input, expected });
  }
  return rows;
}

const wasmBytes = readFileSync(wasmPath);
const golden = parseGolden(readFileSync(goldenPath, "utf8"));

const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const { memory, attestrum_alloc, attestrum_dealloc, attestrum_minhash } = instance.exports;

const enc = new TextEncoder();

// Compute one passage's signature as 128 lowercase 16-char hex strings, driving
// the wasm exactly as the browser does. Re-read memory.buffer after each alloc:
// growing linear memory detaches prior views.
function sigHex(input) {
  const inBytes = enc.encode(input);
  const inPtr = attestrum_alloc(inBytes.length);
  new Uint8Array(memory.buffer, inPtr, inBytes.length).set(inBytes);

  const outPtr = attestrum_alloc(SIG_BYTES);
  const written = attestrum_minhash(inPtr, inBytes.length, outPtr);
  if (written !== SIG_BYTES) {
    console.error(`attestrum_minhash wrote ${written} bytes, want ${SIG_BYTES}`);
    process.exit(1);
  }

  const view = new DataView(memory.buffer, outPtr, SIG_BYTES);
  const hex = [];
  for (let i = 0; i < MINHASH_PERMS; i++) {
    hex.push(view.getBigUint64(i * 8, true).toString(16).padStart(16, "0"));
  }

  attestrum_dealloc(inPtr, inBytes.length);
  attestrum_dealloc(outPtr, SIG_BYTES);
  return hex;
}

let failures = 0;
for (const { label, input, expected } of golden) {
  const got = sigHex(input);
  const firstDiff = got.findIndex((h, i) => h !== expected[i]);
  if (firstDiff !== -1) {
    failures++;
    console.error(
      `MISMATCH on \`${label}\` at perm ${firstDiff}: wasm=${got[firstDiff]} golden=${expected[firstDiff]}`,
    );
  } else {
    console.log(`OK  ${label} (${MINHASH_PERMS} perms byte-identical)`);
  }
}

if (failures > 0) {
  console.error(`\nFAIL — ${failures}/${golden.length} passages diverged from the native golden`);
  process.exit(1);
}
console.log(`\nOK — all ${golden.length} passages: real wasm == native kernel, byte-for-byte`);
