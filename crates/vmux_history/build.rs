use std::path::PathBuf;

use vmux_page::build::PageBuilder;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    PageBuilder::new(manifest_dir, "vmux_history", "vmux_history_app")
        .dx_extra_args(&["--bin", "vmux_history_app", "--features", "web"])
        .track_manifest_rel_paths(&["assets/input.css"])
        .run("vmux_history");
}
