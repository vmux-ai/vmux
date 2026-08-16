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
/// - `web` — wasm32: the page runs inside a browser, so `wasm-bindgen`, `js-sys` and `web_sys`
///   exist. This is the narrowest of the three and the only one that implies a JS runtime.
/// - `ui` — wasm32, iOS or macOS: the Dioxus components. A page's markup and state, which say
///   nothing about who renders them.
/// - `host` — everything but wasm32 and iOS: the desktop app and the daemon. Bevy, the
///   filesystem, and the ability to start a process. Unchanged by macOS joining `ui`.
///
/// **`ui` and `host` overlap, and macOS is where they do.** The desktop runs a page's components
/// in its own process and hands the document to a webview, so it is both at once. `web` implies
/// `ui`; nothing else implies anything.
///
/// So `not(host)` is not a synonym for `ui`, and `ui` is not a synonym for "not the desktop".
/// Gate on `web` when the code needs a browser, on `ui` when it is page code, and on `host` when
/// it needs the machine.
///
/// Cargo cannot read these — dependency resolution runs before any build script — so a manifest
/// spells the same predicate out as `cfg(any(target_arch = "wasm32", target_os = "ios",
/// target_os = "macos"))`.
pub fn emit() {
    println!("cargo::rustc-check-cfg=cfg(host)");
    println!("cargo::rustc-check-cfg=cfg(ui)");
    println!("cargo::rustc-check-cfg=cfg(web)");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let web = arch == "wasm32";
    let phone = os == "ios";

    if web {
        println!("cargo::rustc-cfg=web");
    }
    if web || phone || os == "macos" {
        println!("cargo::rustc-cfg=ui");
    }
    if !web && !phone {
        println!("cargo::rustc-cfg=host");
    }
}
