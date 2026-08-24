#[path = "build_git_env.rs"]
mod build_git_env;
#[path = "build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_git_env::emit();
    build_platform_cfg::emit();
}
