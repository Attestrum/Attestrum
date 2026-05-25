//! Build script: propagate the cargo `TARGET` env var into the compiled
//! binary as `ATTESTRUM_TARGET_TRIPLE` so `attestrum sign` can stamp the predicate's
//! `determinism.targetTriple` field at runtime without any per-platform
//! runtime detection (which can drift between gnu and musl).
//!
//! Cargo sets `TARGET` during the build of build scripts; we re-emit it as
//! a `rustc-env` directive so it appears as `env!("ATTESTRUM_TARGET_TRIPLE")`
//! inside the binary.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=ATTESTRUM_TARGET_TRIPLE={target}");
    // Re-run only if the build script itself changes; TARGET is fixed per
    // cargo invocation.
    println!("cargo:rerun-if-changed=build.rs");
}
