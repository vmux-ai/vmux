use std::path::PathBuf;

use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_status_bar", "vmux_status_bar_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "assets/theme.css"])
        .dx_extra_args(&["--bin", "vmux_status_bar_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxh", "status_bar-dxh"])
        .run("vmux_status_bar");
}
