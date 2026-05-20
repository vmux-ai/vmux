use std::path::PathBuf;

use vmux_page::build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir, "vmux_space", "vmux_space_app")
        .track_manifest_rel_paths(&["../vmux_ui/assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_space_app", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxs", "spaces-dxs"])
        .run("vmux_space");
}
