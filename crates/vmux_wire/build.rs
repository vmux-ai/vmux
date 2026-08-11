#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_platform_cfg::emit();
    emit_bevy_linked();
}

/// Emit `bevy_linked` when the optional `bevy_ecs`/`bevy_reflect` dependencies are actually
/// compiled in.
///
/// `feature = "bevy"` alone does not answer that. The manifest declares both crates for `host`
/// only, while feature unification turns the feature on for every target — so anywhere else the
/// feature is set and the crates are absent, and a `#[cfg(feature = "bevy")]` item there fails to
/// resolve `bevy_reflect`.
///
/// The target test has to match that manifest predicate exactly. Testing for wasm alone was enough
/// until iOS arrived: iOS is not wasm, so the feature went on claiming Bevy was linked for a
/// target that never had it.
fn emit_bevy_linked() {
    println!("cargo::rustc-check-cfg=cfg(bevy_linked)");

    let enabled = std::env::var_os("CARGO_FEATURE_BEVY").is_some();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let host = arch != "wasm32" && os != "ios";
    if enabled && host {
        println!("cargo::rustc-cfg=bevy_linked");
    }
}
