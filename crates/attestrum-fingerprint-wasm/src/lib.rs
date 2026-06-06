//! WASM `extern "C"` wrapper around the PROTECTED text-MinHash kernel
//! (CLAUDE.md §4).
//!
//! This crate compiles [`attestrum_text_minhash`]'s `normalize_text` +
//! [`minhash::compute`](attestrum_text_minhash::minhash::compute) to
//! `wasm32-unknown-unknown` and exposes them through a minimal raw `extern "C"`
//! ABI (no `wasm-bindgen`, so no new dependency). The attestrum.com near-match
//! demo loads the resulting `.wasm` and computes a query's 128-permutation
//! MinHash signature **in the browser, with the identical Rust** that the CLI
//! and the sealed-corpus sidecars use — there is no second implementation that
//! could drift.
//!
//! # No algorithm here
//!
//! Every export delegates straight to `attestrum-text-minhash`. This crate owns
//! only the FFI marshalling (UTF-8 in, 128 little-endian `u64` out). The
//! PROTECTED parameters (NFC / lowercase / whitespace-collapse, 5-gram word
//! shingles, 128 permutations, the BLAKE3 keying scheme) live in the kernel
//! crate and are unchanged. Byte-identity of the wasm output against the native
//! kernel is enforced by the `wasm-crosscheck` CI gate.
//!
//! # ABI
//!
//! The browser glue (and the `tools/wasm-crosscheck/run.mjs` loader) uses:
//!
//! 1. [`attestrum_alloc`] `(len) -> ptr` to reserve `len` bytes of wasm linear
//!    memory and copy the UTF-8 query into it.
//! 2. [`attestrum_minhash`] `(in_ptr, in_len, out_ptr) -> bytes_written` to
//!    compute the signature; it writes exactly `128 * 8 = 1024` bytes
//!    (128 little-endian `u64`) to `out_ptr` and returns `1024`. `out_ptr` must
//!    point at a buffer of at least 1024 bytes (also obtained via
//!    [`attestrum_alloc`]).
//! 3. [`attestrum_dealloc`] `(ptr, len)` to release each buffer afterward.
//!
//! All three are `unsafe`: they dereference caller-supplied raw pointers. The
//! wasm host (JS) upholds the contract; the native cross-check test calls them
//! inside `unsafe` blocks.

use attestrum_text_minhash::{minhash, normalize_text};

/// Number of `u64` permutations in a MinHash signature (PROTECTED, mirrors the
/// kernel's `PERMUTATIONS`). The output buffer must hold `MINHASH_PERMS * 8`
/// bytes.
pub const MINHASH_PERMS: usize = 128;

/// Reserve `len` bytes of wasm linear memory and return a pointer to the start.
///
/// The buffer is leaked from Rust's view so the host can write into it; release
/// it with [`attestrum_dealloc`] using the **same** `len`.
///
/// # Safety
///
/// The returned pointer is valid for `len` bytes until passed to
/// [`attestrum_dealloc`]. Passing a different `len` to dealloc is undefined
/// behavior.
#[no_mangle]
pub unsafe extern "C" fn attestrum_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    // Hand ownership to the host. `with_capacity(len)` allocates capacity == len
    // for `u8`; dealloc reconstructs the Vec with that same capacity.
    core::mem::forget(buf);
    ptr
}

/// Release a buffer previously returned by [`attestrum_alloc`].
///
/// # Safety
///
/// `ptr` must have come from [`attestrum_alloc`] with the identical `len`, and
/// must not be used afterward.
#[no_mangle]
pub unsafe extern "C" fn attestrum_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // Reconstruct with capacity == len (the allocation size requested by
    // `attestrum_alloc`) and length 0, then drop to free.
    let _ = Vec::from_raw_parts(ptr, 0, len);
}

/// Compute the PROTECTED 128-permutation MinHash of a UTF-8 query.
///
/// Reads `in_len` bytes at `in_ptr`, runs the kernel's `normalize_text` →
/// `minhash::compute`, and writes the resulting 128 `u64` as little-endian
/// bytes (`128 * 8 = 1024` bytes total) to `out_ptr`. Returns the number of
/// bytes written (`1024`), or `0` if the input is not valid UTF-8.
///
/// `out_ptr` must point at a writable buffer of at least `MINHASH_PERMS * 8`
/// bytes.
///
/// # Safety
///
/// `in_ptr`/`in_len` must describe an initialized, readable byte range, and
/// `out_ptr` must be writable for at least `MINHASH_PERMS * 8` bytes. Both are
/// typically buffers from [`attestrum_alloc`].
#[no_mangle]
pub unsafe extern "C" fn attestrum_minhash(
    in_ptr: *const u8,
    in_len: usize,
    out_ptr: *mut u8,
) -> usize {
    let input = core::slice::from_raw_parts(in_ptr, in_len);
    let text = match core::str::from_utf8(input) {
        Ok(t) => t,
        Err(_) => return 0,
    };

    let sig = minhash::compute(&normalize_text(text));
    debug_assert_eq!(sig.len(), MINHASH_PERMS);

    let out = core::slice::from_raw_parts_mut(out_ptr, sig.len() * 8);
    for (i, v) in sig.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    sig.len() * 8
}
