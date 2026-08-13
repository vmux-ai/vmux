//! Build script for crates that need only the platform cfg aliases.
//!
//! Named by `build = "../build_cfg.rs"` in each manifest rather than copied into every crate as
//! its own `build.rs`, so the `#[path]`-include boilerplate exists once instead of seventeen
//! times. Crates whose build script does real work — `vmux_page`, `vmux_ui`, `vmux_wire` —
//! keep their own file and include [`build_platform_cfg`] directly.

#[path = "build_platform_cfg.rs"]
mod build_platform_cfg;

fn main() {
    build_platform_cfg::emit();
}
