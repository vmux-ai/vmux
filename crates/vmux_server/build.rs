use std::path::PathBuf;

#[allow(dead_code)]
#[path = "src/build.rs"]
mod page_build;

use page_build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir.clone(), "vmux_server", "vmux_server")
        .track_manifest_rel_paths(&[
            "../vmux_ui/assets/theme.css",
            "../vmux_editor/src",
            "../vmux_command/src",
            "../vmux_core/src",
            "../vmux_git/src",
            "../vmux_history/src",
            "../vmux_layout/src",
            "../vmux_profile/src",
            "../vmux_service/src",
            "../vmux_setting/src",
            "../vmux_space/src",
            "../vmux_team/src",
            "../vmux_terminal/src",
            "../vmux_agent/src",
            "../vmux_wire/src",
        ])
        .dx_extra_args(&["--bin", "vmux_server", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxv", "vmux_server-dxv"])
        .copy_manifest_dir_to_dist("../vmux_terminal/assets/fonts", "assets/fonts")
        .run("vmux_server");
}
