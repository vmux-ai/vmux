//! Platform cfg aliases, so a target test reads as a name rather than a double negative.
//!
//! Shared by every crate that splits along the desktop/frontend line. Defined once because two
//! crates that disagreed about what "not wasm" meant would produce items existing in one and not
//! the other, for a reason no reader of either file could see.
//!
//! Cargo's own `[target.'cfg(...)'.dependencies]` cannot use these — dependency resolution happens
//! before any build script runs — so manifests keep spelling the targets out.

/// Emit `native`, `frontend` and `web` for the target being compiled.
///
/// - `web` — wasm32: the browser bundle.
/// - `frontend` — wasm32 or iOS: the Dioxus pages, with no Bevy and no process access.
/// - `native` — everything else: the desktop app and the daemon.
///
/// `frontend` and `native` are exhaustive and mutually exclusive, so `not(native)` and `frontend`
/// mean the same thing; prefer whichever names the code being gated.
pub fn emit() {
    println!("cargo::rustc-check-cfg=cfg(native)");
    println!("cargo::rustc-check-cfg=cfg(frontend)");
    println!("cargo::rustc-check-cfg=cfg(web)");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let web = arch == "wasm32";
    if web {
        println!("cargo::rustc-cfg=web");
    }
    if web || os == "ios" {
        println!("cargo::rustc-cfg=frontend");
    } else {
        println!("cargo::rustc-cfg=native");
    }
}
