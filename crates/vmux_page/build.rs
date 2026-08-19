use std::path::PathBuf;

#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;
#[allow(dead_code)]
#[path = "src/build.rs"]
mod page_build;

use page_build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    build_platform_cfg::emit();
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir.clone(), "vmux_page", "vmux_page")
        // The flat crates the wasm bundle reaches. `vmux_ui` is tracked unconditionally by
        // `tracked_roots`, and everything under `crates/page` by the bucket below.
        .track_manifest_rel_paths(&[
            "../vmux_core/src",
            "../vmux_macro/src",
            "../vmux_profile/src",
            "../vmux_wire/src",
        ])
        .track_bucket_crates("../page", "src")
        .dx_extra_args(&["--bin", "vmux_page", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxv", "vmux_page-dxv"])
        .copy_manifest_dir_to_dist("../page/vmux_terminal/assets/fonts", "assets/fonts")
        .run("vmux_page");
}
