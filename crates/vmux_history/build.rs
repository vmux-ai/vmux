use std::path::PathBuf;

use vmux_server::build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir, "vmux_history", "vmux_history_app")
        .dx_extra_args(&["--bin", "vmux_history_app", "--features", "web"])
        .track_manifest_rel_paths(&["assets/index.css", "../vmux_ui/assets/theme.css"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxh", "input-dxh"])
        .run("vmux_history");
}
