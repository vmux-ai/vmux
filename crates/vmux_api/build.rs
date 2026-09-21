#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_platform_cfg::emit();
    emit_bevy_linked();
}

fn emit_bevy_linked() {
    println!("cargo::rustc-check-cfg=cfg(bevy_linked)");

    let enabled = std::env::var_os("CARGO_FEATURE_BEVY").is_some();
    let host = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "ios";
    if enabled && host {
        println!("cargo::rustc-cfg=bevy_linked");
    }
}
