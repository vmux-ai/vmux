#[path = "../../build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_platform_cfg::emit();
}
