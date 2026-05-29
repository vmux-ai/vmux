use std::fs;
use std::path::{Path, PathBuf};

use vmux_server::build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap_or_default();
    PageBuilder::new(manifest_dir.clone(), "vmux_app", "vmux_app")
        .track_manifest_rel_paths(&[
            "../vmux_ui/assets/theme.css",
            "../vmux_history/src",
            "../vmux_layout/src",
            "../vmux_service/src",
            "../vmux_setting/src",
            "../vmux_space/src",
            "../vmux_terminal/src",
            "../vmux_terminal/assets/fonts",
        ])
        .dx_extra_args(&["--bin", "vmux_app", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxv", "vmux_app-dxv"])
        .run("vmux_app");
    if target.contains("wasm32") {
        return;
    }
    copy_terminal_fonts(&manifest_dir);
}

fn copy_terminal_fonts(manifest_dir: &Path) {
    let src = manifest_dir.join("../vmux_terminal/assets/fonts");
    let dest = manifest_dir.join("dist/assets/fonts");
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
    if fs::create_dir_all(&dest).is_err() {
        return;
    }
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = fs::copy(&path, dest.join(entry.file_name()));
        }
    }
}
