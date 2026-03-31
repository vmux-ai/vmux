//! Builds **`dist/`** on native targets.
//!
//! wasm32 release → `wasm-bindgen` → optional `wasm-opt` → shell `index.html` → Tailwind + CSS inlining.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("vmux_history should live under workspace crates/");

    if std::env::var("TARGET").unwrap_or_default().contains("wasm32") {
        return;
    }

    for p in tracked_inputs(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    if !needs_history_dist_build(&manifest_dir, &workspace_root) {
        return;
    }

    build_history_dist_wasm_bindgen(&workspace_root, &manifest_dir);
}

fn tracked_inputs(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        manifest_dir.join("src/main.rs"),
        manifest_dir.join("src/app.rs"),
        manifest_dir.join("src/cef.rs"),
        manifest_dir.join("src/payload.rs"),
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("Dioxus.toml"),
        manifest_dir.join("assets/index.html"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("tailwind.config.js"),
        manifest_dir.join("package.json"),
        manifest_dir.join("scripts/inline-history-css.mjs"),
    ];
    let lock = manifest_dir.join("package-lock.json");
    if lock.is_file() {
        v.push(lock);
    }
    v
}

fn newest_mtime(paths: &[PathBuf]) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    for p in paths {
        let Ok(meta) = fs::metadata(p) else {
            continue;
        };
        let Ok(t) = meta.modified() else {
            continue;
        };
        newest = Some(match newest {
            Some(n) => n.max(t),
            None => t,
        });
    }
    newest
}

fn needs_history_dist_build(manifest_dir: &Path, workspace_root: &Path) -> bool {
    needs_history_dist_wasm_bindgen(manifest_dir, workspace_root)
}

/// `dist/` must include Tailwind-inlined `index.html` (see `scripts/inline-history-css.mjs`).
/// Otherwise CEF often loads only the shell document and the separate `history.css` request fails,
/// which looks like “no CSS” after restart when stale `dist/` shadows the embedded bundle.
fn history_dist_css_bundle_complete(dist: &Path) -> bool {
    let index = dist.join("index.html");
    let history_css = dist.join("history.css");
    if !history_css.is_file() {
        return false;
    }
    let Ok(html) = fs::read_to_string(&index) else {
        return false;
    };
    html.contains("vmux-history-inline")
}

fn needs_history_dist_wasm_bindgen(manifest_dir: &Path, workspace_root: &Path) -> bool {
    let dist = manifest_dir.join("dist");
    let wasm_out = dist.join("vmux_history_bg.wasm");
    let index = dist.join("index.html");
    if !wasm_out.is_file() || !index.is_file() {
        return true;
    }
    if !history_dist_css_bundle_complete(&dist) {
        return true;
    }
    let Ok(wasm_mtime) = fs::metadata(&wasm_out).and_then(|m| m.modified()) else {
        return true;
    };
    let inputs: Vec<PathBuf> = tracked_inputs(manifest_dir)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    let Some(newest) = newest_mtime(&inputs) else {
        return true;
    };
    if newest > wasm_mtime {
        return true;
    }
    // `cargo clean` or a manual wasm build can leave `dist/` older than `target/.../vmux_history.wasm`.
    let built_wasm = workspace_root.join("target/wasm32-unknown-unknown/release/vmux_history.wasm");
    if built_wasm.is_file() {
        if let (Ok(built_t), Ok(out_t)) = (
            fs::metadata(&built_wasm).and_then(|m| m.modified()),
            fs::metadata(&wasm_out).and_then(|m| m.modified()),
        ) {
            if built_t > out_t {
                return true;
            }
        }
    }
    false
}

fn build_history_dist_wasm_bindgen(workspace_root: &Path, manifest_dir: &Path) {
    let cargo = std::env::var_os("CARGO").expect("CARGO must be set for build scripts");
    let status = Command::new(cargo)
        .env_remove("CEF_PATH")
        .current_dir(workspace_root)
        .args([
            "build",
            "-p",
            "vmux_history",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status()
        .unwrap_or_else(|e| panic!("vmux_history: failed to spawn cargo for wasm build: {e}"));
    if !status.success() {
        panic!("vmux_history: `cargo build -p vmux_history --target wasm32-unknown-unknown --release` failed");
    }

    let wasm = workspace_root.join("target/wasm32-unknown-unknown/release/vmux_history.wasm");
    if !wasm.is_file() {
        panic!(
            "vmux_history: missing {} — wasm build did not produce vmux_history.wasm",
            wasm.display()
        );
    }

    let dist = manifest_dir.join("dist");
    fs::create_dir_all(&dist).unwrap_or_else(|e| {
        panic!(
            "vmux_history: failed to create {}: {e}",
            dist.display()
        )
    });

    let status = Command::new("wasm-bindgen")
        .current_dir(workspace_root)
        .args([
            "target/wasm32-unknown-unknown/release/vmux_history.wasm",
            "--out-dir",
            "crates/vmux_history/dist",
            "--target",
            "web",
            "--no-typescript",
        ])
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "vmux_history: failed to run wasm-bindgen ({e}). Install a CLI version matching the crate's wasm-bindgen dependency (see crates/vmux_history/Cargo.toml)."
            )
        });
    if !status.success() {
        panic!("vmux_history: wasm-bindgen failed");
    }

    let bg = dist.join("vmux_history_bg.wasm");
    if bg.is_file() {
        let _ = Command::new("wasm-opt")
            .arg("-Oz")
            .arg(&bg)
            .arg("-o")
            .arg(&bg)
            .status();
    }

    let shell = manifest_dir.join("assets/index.html");
    let out_index = dist.join("index.html");
    fs::copy(&shell, &out_index).unwrap_or_else(|e| {
        panic!(
            "vmux_history: failed to copy {} to {}: {e}",
            shell.display(),
            out_index.display()
        )
    });

    run_npm_install(manifest_dir);
    run_npm_build_css(manifest_dir);
}

fn run_npm_install(manifest_dir: &Path) {
    let npm_ok = Command::new("npm")
        .args(["install"])
        .current_dir(manifest_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if npm_ok {
        return;
    }
    let bun = Command::new("bun")
        .args(["install"])
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|e| panic!("vmux_history: failed to run bun install: {e}"));
    if !bun.success() {
        panic!("vmux_history: install node deps failed (tried `npm install`, then `bun install`)");
    }
}

fn run_npm_build_css(manifest_dir: &Path) {
    let npm_ok = Command::new("npm")
        .args(["run", "build:css"])
        .current_dir(manifest_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if npm_ok {
        return;
    }
    let bun = Command::new("bun")
        .args(["run", "build:css"])
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|e| panic!("vmux_history: failed to run bun run build:css: {e}"));
    if !bun.success() {
        panic!("vmux_history: Tailwind build failed (tried `npm run build:css`, then `bun run build:css`)");
    }
}
