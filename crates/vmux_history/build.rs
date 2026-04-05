//! Native builds refresh **`dist/`** via **`dx build --platform web`**.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vmux_utils::{
    dx_web_public_dir, replace_dist_from_dx_public, run_dx_web_bundle,
    workspace_root_from_manifest_dir,
};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = workspace_root_from_manifest_dir(&manifest_dir);

    for p in tracked_paths(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("TARGET")
        .unwrap_or_default()
        .contains("wasm32")
    {
        return;
    }

    let release = std::env::var("PROFILE").unwrap_or_default() == "release";
    let dist = manifest_dir.join("dist");
    if needs_dist_rebuild(&manifest_dir, &workspace_root, release) {
        run_dx_web_bundle(
            &workspace_root,
            "vmux_history",
            release,
            &["--bin", "vmux_history_app", "--features", "web"],
        );
        let public = dx_web_public_dir(&workspace_root, "vmux_history_app", release);
        let shell = manifest_dir.join("assets/index.html");
        replace_dist_from_dx_public(&public, &dist, &shell);
    }
    if let Err(e) = copy_theme_css_for_embedded_import(&dist, &workspace_root) {
        println!("cargo:warning=vmux_history: could not copy theme.css for embedded @import: {e}");
    }

    if let Ok(rd) = fs::read_dir(&dist) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }
    let wasm_dir = dist.join("wasm");
    if wasm_dir.is_dir() {
        if let Ok(rd) = fs::read_dir(&wasm_dir) {
            for e in rd.flatten() {
                println!("cargo:rerun-if-changed={}", e.path().display());
            }
        }
    }
    let assets_dir = dist.join("assets");
    if assets_dir.is_dir() {
        if let Ok(rd) = fs::read_dir(&assets_dir) {
            for e in rd.flatten() {
                println!("cargo:rerun-if-changed={}", e.path().display());
            }
        }
    }
}

fn copy_theme_css_for_embedded_import(dist: &Path, workspace_root: &Path) -> std::io::Result<()> {
    if !dist.is_dir() {
        return Ok(());
    }
    let src = workspace_root.join("crates/vmux_ui/assets/theme.css");
    if !src.is_file() {
        return Ok(());
    }
    let dest = dist.join("vmux_ui/assets/theme.css");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dest)?;
    println!("cargo:rerun-if-changed={}", dest.display());
    Ok(())
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
        manifest_dir.join("assets/index.html"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("../vmux_ui/assets/theme.css"),
    ];
    collect_rs_files(&manifest_dir.join("src"), &mut v);
    v.sort();
    v.dedup();
    v
}

fn dist_dependency_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = tracked_paths(manifest_dir);
    v.push(manifest_dir.join("build.rs"));
    v
}

fn newest_bg_wasm_mtime(dir: &Path) -> Option<SystemTime> {
    let wasm_dir = dir.join("wasm");
    if !wasm_dir.is_dir() {
        return None;
    }
    let mut newest: Option<SystemTime> = None;
    let rd = fs::read_dir(&wasm_dir).ok()?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with("_bg.wasm") {
            if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                newest = Some(newest.map_or(t, |n: SystemTime| n.max(t)));
            }
        }
    }
    newest
}

fn needs_dist_rebuild(manifest_dir: &Path, workspace_root: &Path, release: bool) -> bool {
    let dist = manifest_dir.join("dist");
    let index = dist.join("index.html");
    let Some(wasm_mtime) = newest_bg_wasm_mtime(&dist) else {
        return true;
    };
    if !index.is_file() {
        return true;
    }
    for p in dist_dependency_paths(manifest_dir) {
        if let Ok(t) = fs::metadata(&p).and_then(|m| m.modified()) {
            if t > wasm_mtime {
                return true;
            }
        }
    }
    let dx_public = dx_web_public_dir(workspace_root, "vmux_history_app", release);
    if let Some(dx_mtime) = newest_bg_wasm_mtime(&dx_public) {
        if dx_mtime > wasm_mtime {
            return true;
        }
    }
    false
}
