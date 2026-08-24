#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    build_platform_cfg::emit();
}
