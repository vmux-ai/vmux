//! Native builds refresh **`dist/`** via **`dx build --platform web`** (Tailwind + wasm tooling via dioxus-cli).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use vmux_utils::{
    dx_web_public_dir, replace_dist_from_dx_public, run_dx_web_bundle, workspace_root_from_manifest_dir,
};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = workspace_root_from_manifest_dir(&manifest_dir);

    if std::env::var("TARGET")
        .unwrap_or_default()
        .contains("wasm32")
    {
        return;
    }

    for p in tracked_inputs(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let dx_release = true;
    if !needs_history_dist_build(&manifest_dir, &workspace_root, dx_release) {
        return;
    }

    run_dx_web_bundle(&workspace_root, "vmux_history", dx_release, &[]);
    let public = dx_web_public_dir(&workspace_root, "vmux_history", dx_release);
    let dist = manifest_dir.join("dist");
    let shell = manifest_dir.join("assets/index.html");
    replace_dist_from_dx_public(&public, &dist, &shell);
}

fn tracked_inputs(manifest_dir: &Path) -> Vec<PathBuf> {
    let workspace_root = workspace_root_from_manifest_dir(manifest_dir);
    vec![
        manifest_dir.join("src/main.rs"),
        manifest_dir.join("src/app.rs"),
        manifest_dir.join("src/cef.rs"),
        manifest_dir.join("src/payload.rs"),
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("Dioxus.toml"),
        manifest_dir.join("assets/index.html"),
        manifest_dir.join("assets/input.css"),
        manifest_dir.join("tailwind.config.js"),
        workspace_root.join("crates/vmux_ui/assets/theme.css"),
        workspace_root.join("crates/vmux_ui/tailwind.preset.js"),
    ]
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

fn needs_history_dist_build(
    manifest_dir: &Path,
    workspace_root: &Path,
    dx_release: bool,
) -> bool {
    let dist = manifest_dir.join("dist");
    let wasm_out = dist.join("vmux_history_bg.wasm");
    let index = dist.join("index.html");
    if !wasm_out.is_file() || !index.is_file() {
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
    let dx_public = dx_web_public_dir(workspace_root, "vmux_history", dx_release);
    let dx_wasm = dx_public.join("wasm").join("vmux_history_bg.wasm");
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
