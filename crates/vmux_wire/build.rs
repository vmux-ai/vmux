#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_platform_cfg::emit();
    emit_bevy_linked();
}

/// Emit `bevy_linked` when the optional `bevy_ecs`/`bevy_reflect` dependencies are actually
/// compiled in.
///
/// `feature = "bevy"` alone does not answer that. The manifest declares both crates under
/// `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, while feature unification turns the
/// feature on for the wasm bundle as well — so on wasm the feature is set and the crates are
/// absent, and a `#[cfg(feature = "bevy")]` item there fails to resolve `bevy_reflect`.
fn emit_bevy_linked() {
    println!("cargo::rustc-check-cfg=cfg(bevy_linked)");

    let enabled = std::env::var_os("CARGO_FEATURE_BEVY").is_some();
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "wasm32";
    if enabled && !wasm {
        println!("cargo::rustc-cfg=bevy_linked");
    }
}
