//! Debug **native** builds refresh **`dist/`** via **`dx build --platform web`** (`--no-default-features` for the wasm binary).
//! **Release** native builds skip the UI library bundle. **Wasm** crate builds are a no-op here.

use std::fs;
use std::path::{Path, PathBuf};
use vmux_webview_app::build::{
    dx_web_public_dir, replace_dist_from_dx_public, run_dx_web_bundle,
    workspace_root_from_manifest_dir,
};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = workspace_root_from_manifest_dir(&manifest_dir);

    println!("cargo:rerun-if-changed=build.rs");

    let target = std::env::var("TARGET").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();

    if target.contains("wasm32") {
        for p in tracked_paths(&manifest_dir) {
            println!("cargo:rerun-if-changed={}", p.display());
        }
        return;
    }

    if profile == "release" {
        return;
    }

    for p in tracked_paths(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    // Match prior `cargo build … --release` for the wasm gallery bundle.
    let dx_release = true;
    if !needs_dist_rebuild(&manifest_dir, dx_release) {
        return;
    }

    run_dx_web_bundle(
        &workspace_root,
        "vmux_ui",
        dx_release,
        &["--no-default-features"],
    );
    let public = dx_web_public_dir(&workspace_root, "vmux_ui", dx_release);
    let dist = manifest_dir.join("dist");
    let shell = manifest_dir.join("assets/index.html");
    replace_dist_from_dx_public(&public, &dist, &shell);
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn tracked_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("Dioxus.toml"),
        manifest_dir.join("tailwind.config.js"),
        manifest_dir.join("tailwind.preset.js"),
        manifest_dir.join("assets/index.html"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("assets/theme.css"),
    ];
    collect_rs_files(&manifest_dir.join("src").join("gallery"), &mut v);
    collect_rs_files(&manifest_dir.join("src").join("components"), &mut v);
    collect_rs_files(&manifest_dir.join("src").join("hooks"), &mut v);
    for f in [
        "main.rs",
        "lib.rs",
        "server.rs",
        "ui.rs",
        "components.rs",
        "util.rs",
        "cef_bridge.rs",
    ] {
        v.push(manifest_dir.join("src").join(f));
    }
    v.sort();
    v.dedup();
    v
}

fn dist_dependency_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = tracked_paths(manifest_dir);
    v.push(manifest_dir.join("build.rs"));
    v
}

fn needs_dist_rebuild(manifest_dir: &Path, dx_release: bool) -> bool {
    let dist = manifest_dir.join("dist");
    let wasm_out = dist.join("vmux_ui_bg.wasm");
    let index = dist.join("index.html");
    if !wasm_out.is_file() || !index.is_file() {
        return true;
    }
    let Ok(wasm_mtime) = fs::metadata(&wasm_out).and_then(|m| m.modified()) else {
        return true;
    };
    for p in dist_dependency_paths(manifest_dir) {
        if let Ok(t) = fs::metadata(&p).and_then(|m| m.modified()) {
            if t > wasm_mtime {
                return true;
            }
        }
    }
    let workspace_root = workspace_root_from_manifest_dir(manifest_dir);
    let dx_public = dx_web_public_dir(&workspace_root, "vmux_ui", dx_release);
    let dx_wasm = dx_public.join("wasm").join("vmux_ui_bg.wasm");
    if dx_wasm.is_file() {
        if let (Ok(dx_t), Ok(dist_t)) = (
            fs::metadata(&dx_wasm).and_then(|m| m.modified()),
            fs::metadata(&wasm_out).and_then(|m| m.modified()),
        ) && dx_t > dist_t
        {
            return true;
        }
    }
    false
}
