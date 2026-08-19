//! Platform cfg aliases, so a target test reads as a name rather than a double negative.
//!
//! Shared by every crate that splits along the host/ui line. Defined once because two crates that
//! disagreed about what "not iOS" meant would produce items existing in one and not the other,
//! for a reason no reader of either file could see.
//!
//! Cargo's own `[target.'cfg(...)'.dependencies]` cannot use these — dependency resolution happens
//! before any build script runs — so manifests keep spelling the targets out.

/// Emit `host` and `ui` for the target being compiled.
///
/// - `ui` — iOS or macOS: the Dioxus components. A page's markup and state, which say nothing
///   about who renders them.
/// - `host` — everything but iOS: the desktop app and the daemon. Bevy, the filesystem, and the
///   ability to start a process.
///
/// **`ui` and `host` overlap, and macOS is where they do.** The desktop runs a page's components
/// in its own process and hands the document to a webview, so it is both at once.
///
/// There was a third, `web`, for when pages were compiled to wasm and served into a browser. No
/// page is, so nothing is built for wasm32 and the alias had no target left to name.
///
/// Cargo cannot read these — dependency resolution runs before any build script — so a manifest
/// spells the same predicate out as `cfg(any(target_os = "ios", target_os = "macos"))`.
pub fn emit() {
    println!("cargo::rustc-check-cfg=cfg(host)");
    println!("cargo::rustc-check-cfg=cfg(ui)");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let phone = os == "ios";

    if phone || os == "macos" {
        println!("cargo::rustc-cfg=ui");
    }
    if !phone {
        println!("cargo::rustc-cfg=host");
    }
}
