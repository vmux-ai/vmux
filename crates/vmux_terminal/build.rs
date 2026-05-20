use std::path::PathBuf;

use vmux_server::build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir, "vmux_terminal", "vmux_terminal_app")
        .track_manifest_rel_paths(&["../vmux_ui/assets/theme.css", "assets/fonts"])
        .dx_extra_args(&["--bin", "vmux_terminal_app", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxh", "terminal-dxh"])
        .run("vmux_terminal");
}
