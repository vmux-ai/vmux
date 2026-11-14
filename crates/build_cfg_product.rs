#[path = "build_git_env.rs"]
mod build_git_env;
#[path = "build_platform_cfg.rs"]
mod build_platform_cfg;
#[path = "build_product.rs"]
pub mod build_product;

fn main() {
    build_git_env::emit();
    build_platform_cfg::emit();
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..");
    build_product::ProductMetadata::current(&root)
        .unwrap_or_else(|error| panic!("Invalid product host configuration: {error}"))
        .emit();
}
