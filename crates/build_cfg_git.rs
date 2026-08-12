//! Build script for crates that need the platform cfg aliases and the git build stamp.
//!
//! The git half emits `rerun-if-changed` on the repo's HEAD refs, so it is kept off the crates
//! that do not read `VMUX_GIT_HASH` — those use [`build_cfg`](../build_cfg.rs) instead.

#[path = "build_git_env.rs"]
mod build_git_env;
#[path = "build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_git_env::emit();
    build_platform_cfg::emit();
}
