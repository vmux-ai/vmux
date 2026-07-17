//! Native debug builds with `--features gallery` refresh **`dist/`** via
//! **`dx build --platform web`** (`--no-default-features` for the wasm binary).
//! Release native builds and wasm crate builds are no-ops here.

#[cfg(feature = "gallery")]
use std::fs;
#[cfg(feature = "gallery")]
use std::path::{Path, PathBuf};

#[cfg(feature = "gallery")]
#[allow(dead_code)]
#[path = "../vmux_server/src/build.rs"]
mod page_build;

#[cfg(feature = "gallery")]
use page_build::{
    SKIP_DX_BUILD_ENV, dx_web_public_dir, replace_dist_from_dx_public, run_dx_web_bundle,
    skip_dx_build, workspace_root_from_manifest_dir,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "gallery")]
    build_gallery();
}

#[cfg(feature = "gallery")]
fn build_gallery() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = workspace_root_from_manifest_dir(&manifest_dir);

    println!("cargo:rerun-if-changed=../vmux_server/src/build.rs");
    println!("cargo:rerun-if-env-changed={SKIP_DX_BUILD_ENV}");

    let target = std::env::var("TARGET").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();

    if target.contains("wasm32") {
        return;
    }

    if profile == "release" {
        return;
    }

    if skip_dx_build() {
        return;
    }

    let tracked_paths = tracked_paths(&manifest_dir);
    for p in &tracked_paths {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    // Match prior `cargo build … --release` for the wasm gallery bundle.
    let dx_release = true;
    if !needs_dist_rebuild(&manifest_dir, dx_release, &tracked_paths) {
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

#[cfg(feature = "gallery")]
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

#[cfg(feature = "gallery")]
fn tracked_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let workspace_root = workspace_root_from_manifest_dir(manifest_dir);
    let mut v = vec![
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("Dioxus.toml"),
        manifest_dir.join("assets/index.html"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("assets/theme.css"),
        workspace_root.join("Cargo.toml"),
        workspace_root.join("Cargo.lock"),
    ];
    collect_files(&manifest_dir.join("src"), &mut v);
    v.sort();
    v.dedup();
    v
}

#[cfg(feature = "gallery")]
fn needs_dist_rebuild(manifest_dir: &Path, dx_release: bool, tracked_paths: &[PathBuf]) -> bool {
    let dist = manifest_dir.join("dist");
    let wasm_out = dist.join("vmux_ui_bg.wasm");
    let index = dist.join("index.html");
    if !wasm_out.is_file() || !index.is_file() {
        return true;
    }
    let Ok(wasm_mtime) = fs::metadata(&wasm_out).and_then(|m| m.modified()) else {
        return true;
    };
    let build_script = manifest_dir.join("build.rs");
    for p in tracked_paths.iter().chain(std::iter::once(&build_script)) {
        if let Ok(t) = fs::metadata(p).and_then(|m| m.modified())
            && t > wasm_mtime
        {
            return true;
        }
    }
    let workspace_root = workspace_root_from_manifest_dir(manifest_dir);
    let dx_public = dx_web_public_dir(&workspace_root, "vmux_ui", dx_release);
    let dx_wasm = dx_public.join("wasm").join("vmux_ui_bg.wasm");
    if dx_wasm.is_file()
        && let (Ok(dx_t), Ok(dist_t)) = (
            fs::metadata(&dx_wasm).and_then(|m| m.modified()),
            fs::metadata(&wasm_out).and_then(|m| m.modified()),
        )
        && dx_t > dist_t
    {
        return true;
    }
    false
}
