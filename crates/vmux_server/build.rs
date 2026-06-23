use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[path = "src/build.rs"]
mod page_build;

use page_build::{CefEmbeddedPageFinalize, PageBuilder};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap_or_default();
    PageBuilder::new(manifest_dir.clone(), "vmux_server", "vmux_server")
        .track_manifest_rel_paths(&[
            "../vmux_ui/assets/theme.css",
            "../vmux_ui/src",
            "../vmux_editor/src",
            "../vmux_git/src",
            "../vmux_history/src",
            "../vmux_layout/src",
            "../vmux_service/src",
            "../vmux_setting/src",
            "../vmux_space/src",
            "../vmux_terminal/src",
            "../vmux_terminal/assets/fonts",
            "../vmux_vibe_setup/src",
        ])
        .dx_extra_args(&["--bin", "vmux_server", "--features", "web"])
        .cef_finalize(CefEmbeddedPageFinalize {
            strip_uncompiled_tailwind_css: true,
        })
        .tailwind_postprocess_after_dx(&["index-dxv", "vmux_server-dxv"])
        .run("vmux_server");
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
