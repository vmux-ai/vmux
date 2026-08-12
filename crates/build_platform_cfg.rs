//! Platform cfg aliases, so a target test reads as a name rather than a double negative.
//!
//! Shared by every crate that splits along the host/ui line. Defined once because two crates that
//! disagreed about what "not wasm" meant would produce items existing in one and not the other,
//! for a reason no reader of either file could see.
//!
//! Cargo's own `[target.'cfg(...)'.dependencies]` cannot use these — dependency resolution happens
//! before any build script runs — so manifests keep spelling the targets out.

/// Emit `host`, `ui` and `web` for the target being compiled.
///
/// - `web` — wasm32: the browser bundle.
/// - `ui` — wasm32 or iOS: the Dioxus pages, with no Bevy and no process access.
/// - `host` — everything else: the desktop app and the daemon.
///
/// `ui` and `host` are exhaustive and mutually exclusive, so `not(host)` and `ui` mean the same
/// thing; prefer whichever names the code being gated.
///
/// iOS is native code but is not `host`: it runs the pages, so it is `ui`.
pub fn emit() {
    println!("cargo::rustc-check-cfg=cfg(host)");
    println!("cargo::rustc-check-cfg=cfg(ui)");
    println!("cargo::rustc-check-cfg=cfg(web)");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let web = arch == "wasm32";
    if web {
        println!("cargo::rustc-cfg=web");
    }
    if web || os == "ios" {
        println!("cargo::rustc-cfg=ui");
    } else {
        println!("cargo::rustc-cfg=host");
    }
}
