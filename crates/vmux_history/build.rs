use std::path::PathBuf;

use vmux_webview_app::build::WebviewAppBuilder;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    WebviewAppBuilder::new(manifest_dir, "vmux_history", "vmux_history_app")
        .dx_extra_args(&["--bin", "vmux_history_app", "--features", "web"])
        .track_manifest_rel_paths(&["tailwind.config.js", "assets/input.css"])
        .run("vmux_history");
}
