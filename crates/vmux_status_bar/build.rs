//! Native builds refresh **`dist/`**: wasm32 release → `wasm-bindgen` → optional `wasm-opt` → shell **`assets/index.html`**.
//! Same idea as `crates/vmux_history/build.rs` (no `dioxus-cli` / `dx` binary).
//!
//! **Wasm** builds only ensure **`assets/status.css`** exists; `app.rs` uses `include_str!` for it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("vmux_status_bar should live under workspace crates/");

    for p in tracked_paths(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("wasm32") {
        ensure_tailwind_css(&manifest_dir);
        return;
    }

    if needs_dist_rebuild(&manifest_dir) {
        build_status_dist(&workspace_root, &manifest_dir);
    }
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
    let mut v = tailwind_css_inputs(manifest_dir);
    v.push(manifest_dir.join("Cargo.toml"));
    v.push(manifest_dir.join("assets/index.html"));
    v.push(manifest_dir.join("assets/status.css"));
    v.sort();
    v.dedup();
    v
}

fn dist_dependency_paths(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("package.json"),
        manifest_dir.join("tailwind.config.js"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("assets/status.css"),
        manifest_dir.join("assets/index.html"),
    ];
    let lock = manifest_dir.join("package-lock.json");
    if lock.is_file() {
        v.push(lock);
    }
    for f in ["main.rs", "app.rs", "bridge.rs", "payload.rs"] {
        v.push(manifest_dir.join("src").join(f));
    }
    v
}

fn tailwind_css_inputs(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        manifest_dir.join("package.json"),
        manifest_dir.join("tailwind.config.js"),
        manifest_dir.join("assets/input.css"),
    ];
    let lock = manifest_dir.join("package-lock.json");
    if lock.is_file() {
        v.push(lock);
    }
    collect_rs_files(&manifest_dir.join("src"), &mut v);
    v
}

fn needs_tailwind_refresh(manifest_dir: &Path) -> bool {
    let css = manifest_dir.join("assets/status.css");
    if !css.is_file() {
        return true;
    }
    let Ok(css_t) = fs::metadata(&css).and_then(|m| m.modified()) else {
        return true;
    };
    for p in tailwind_css_inputs(manifest_dir) {
        if let Ok(t) = fs::metadata(&p).and_then(|m| m.modified()) {
            if t > css_t {
                return true;
            }
        }
    }
    false
}

fn ensure_tailwind_css(manifest_dir: &Path) {
    if !needs_tailwind_refresh(manifest_dir) {
        return;
    }
    run_npm_install(manifest_dir);
    run_npm_build_css(manifest_dir);
}

fn needs_dist_rebuild(manifest_dir: &Path) -> bool {
    let dist = manifest_dir.join("dist");
    let wasm_out = dist.join("vmux_status_bar_bg.wasm");
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
    false
}

fn run_npm_install(manifest_dir: &Path) {
    let status = Command::new("npm")
        .args(["install"])
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|e| panic!("vmux_status_bar: failed to run npm install: {e}"));
    if !status.success() {
        panic!("vmux_status_bar: npm install failed");
    }
}

fn run_npm_build_css(manifest_dir: &Path) {
    let status = Command::new("npm")
        .args(["run", "build:css"])
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|e| panic!("vmux_status_bar: failed to run npm run build:css: {e}"));
    if !status.success() {
        panic!("vmux_status_bar: npm run build:css failed");
    }
}

fn build_status_dist(workspace_root: &Path, manifest_dir: &Path) {
    run_npm_install(manifest_dir);
    run_npm_build_css(manifest_dir);

    let cargo = std::env::var_os("CARGO").expect("CARGO must be set for build scripts");
    let status = Command::new(&cargo)
        .env_remove("CEF_PATH")
        .current_dir(workspace_root)
        .args([
            "build",
            "-p",
            "vmux_status_bar",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status()
        .unwrap_or_else(|e| panic!("vmux_status_bar: failed to spawn cargo for wasm build: {e}"));
    if !status.success() {
        panic!("vmux_status_bar: `cargo build -p vmux_status_bar --target wasm32-unknown-unknown --release` failed");
    }

    let wasm = workspace_root.join("target/wasm32-unknown-unknown/release/vmux_status_bar.wasm");
    if !wasm.is_file() {
        panic!(
            "vmux_status_bar: missing {} — wasm build did not produce vmux_status_bar.wasm",
            wasm.display()
        );
    }

    let dist = manifest_dir.join("dist");
    let _ = fs::remove_dir_all(&dist);
    fs::create_dir_all(&dist).unwrap_or_else(|e| {
        panic!(
            "vmux_status_bar: failed to create {}: {e}",
            dist.display()
        )
    });

    let status = Command::new("wasm-bindgen")
        .current_dir(workspace_root)
        .args([
            "target/wasm32-unknown-unknown/release/vmux_status_bar.wasm",
            "--out-dir",
            "crates/vmux_status_bar/dist",
            "--target",
            "web",
            "--no-typescript",
        ])
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "vmux_status_bar: failed to run wasm-bindgen ({e}). Install a CLI version matching the `wasm-bindgen` dependency pulled in by Dioxus (see Cargo.lock)."
            )
        });
    if !status.success() {
        panic!("vmux_status_bar: wasm-bindgen failed");
    }

    let bg = dist.join("vmux_status_bar_bg.wasm");
    if bg.is_file() {
        let _ = Command::new("wasm-opt")
            .arg("-Oz")
            .arg(&bg)
            .arg("-o")
            .arg(&bg)
            .status();
    }

    let shell = manifest_dir.join("assets/index.html");
    if !shell.is_file() {
        panic!(
            "vmux_status_bar: missing shell HTML at {}",
            shell.display()
        );
    }
    fs::copy(&shell, dist.join("index.html")).unwrap_or_else(|e| {
        panic!(
            "vmux_status_bar: failed to copy {} to dist/index.html: {e}",
            shell.display()
        )
    });
}
