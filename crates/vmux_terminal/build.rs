use std::path::PathBuf;

use vmux_webview_app::build::{CefEmbeddedWebviewFinalize, WebviewAppBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_terminal", "vmux_terminal_app")
        .track_manifest_rel_paths(&["tailwind.config.js", "../vmux_ui/assets/theme.css", "assets/fonts"])
        .dx_extra_args(&["--bin", "vmux_terminal_app", "--features", "web"])
        .cef_finalize(CefEmbeddedWebviewFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxh", "terminal-dxh"])
        .run("vmux_terminal");
}
