//! Builds **`web_dist/`** on native targets.
//!
//! **Default:** wasm32 release → `wasm-bindgen` → optional `wasm-opt` → shell `index.html` → Tailwind + CSS inlining.
//!
//! **Optional:** set **`VMUX_HISTORY_USE_DX=1`** to use the Dioxus CLI instead (`dx build` → `dist/` → `web_dist/` → Tailwind),
//! matching the former `make history-ui-dx` recipe.

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

    println!("cargo:rerun-if-env-changed=VMUX_HISTORY_USE_DX");
    let use_dx = history_use_dx();

    for p in tracked_inputs(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    if !needs_history_web_dist_build(&manifest_dir, use_dx, &workspace_root) {
        return;
    }

    if use_dx {
        build_history_web_dist_dx(&manifest_dir);
    } else {
        build_history_web_dist_wasm_bindgen(&workspace_root, &manifest_dir);
    }
}

fn history_use_dx() -> bool {
    std::env::var("VMUX_HISTORY_USE_DX")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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

fn dir_contains_wasm(dir: &Path) -> bool {
    fn walk(p: &Path) -> bool {
        let Ok(rd) = fs::read_dir(p) else {
            return false;
        };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                if walk(&path) {
                    return true;
                }
            } else if path.extension().is_some_and(|x| x == "wasm") {
                return true;
            }
        }
        false
    }
    walk(dir)
}

fn needs_history_web_dist_build(manifest_dir: &Path, use_dx: bool, workspace_root: &Path) -> bool {
    if use_dx {
        needs_history_web_dist_dx(manifest_dir)
    } else {
        needs_history_web_dist_wasm_bindgen(manifest_dir, workspace_root)
    }
}

/// `web_dist/` must include Tailwind-inlined `index.html` (see `scripts/inline-history-css.mjs`).
/// Otherwise CEF often loads only the shell document and the separate `history.css` request fails,
/// which looks like “no CSS” after restart when stale `web_dist/` shadows the embedded bundle.
fn history_web_dist_css_bundle_complete(web: &Path) -> bool {
    let index = web.join("index.html");
    let history_css = web.join("history.css");
    if !history_css.is_file() {
        return false;
    }
    let Ok(html) = fs::read_to_string(&index) else {
        return false;
    };
    html.contains("vmux-history-inline")
}

fn needs_history_web_dist_wasm_bindgen(manifest_dir: &Path, workspace_root: &Path) -> bool {
    let web = manifest_dir.join("web_dist");
    let wasm_out = web.join("vmux_history_bg.wasm");
    let index = web.join("index.html");
    if !wasm_out.is_file() || !index.is_file() {
        return true;
    }
    if !history_web_dist_css_bundle_complete(&web) {
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
    // `cargo clean` or a manual wasm build can leave `web_dist/` older than `target/.../vmux_history.wasm`.
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

fn needs_history_web_dist_dx(manifest_dir: &Path) -> bool {
    let web = manifest_dir.join("web_dist");
    let index = web.join("index.html");
    if !index.is_file() || !dir_contains_wasm(&web) {
        return true;
    }
    if !history_web_dist_css_bundle_complete(&web) {
        return true;
    }
    let Ok(stamp) = fs::metadata(&index).and_then(|m| m.modified()) else {
        return true;
    };
    for p in tracked_inputs(manifest_dir) {
        if let Ok(t) = fs::metadata(p).and_then(|m| m.modified()) {
            if t > stamp {
                return true;
            }
        }
    }
    false
}

fn build_history_web_dist_wasm_bindgen(workspace_root: &Path, manifest_dir: &Path) {
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

    let web_dist = manifest_dir.join("web_dist");
    fs::create_dir_all(&web_dist).unwrap_or_else(|e| {
        panic!(
            "vmux_history: failed to create {}: {e}",
            web_dist.display()
        )
    });

    let status = Command::new("wasm-bindgen")
        .current_dir(workspace_root)
        .args([
            "target/wasm32-unknown-unknown/release/vmux_history.wasm",
            "--out-dir",
            "crates/vmux_history/web_dist",
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

    let bg = web_dist.join("vmux_history_bg.wasm");
    if bg.is_file() {
        let _ = Command::new("wasm-opt")
            .arg("-Oz")
            .arg(&bg)
            .arg("-o")
            .arg(&bg)
            .status();
    }

    let shell = manifest_dir.join("assets/index.html");
    let out_index = web_dist.join("index.html");
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

fn dioxus_application_name(manifest_dir: &Path) -> String {
    let path = manifest_dir.join("Dioxus.toml");
    let Ok(s) = fs::read_to_string(&path) else {
        return "vmux_history".to_string();
    };
    let mut in_application = false;
    for line in s.lines() {
        let t = line.trim();
        if t == "[application]" {
            in_application = true;
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            in_application = false;
            continue;
        }
        if in_application {
            if let Some(v) = t.strip_prefix("name = ") {
                return v.trim().trim_matches('"').to_string();
            }
        }
    }
    "vmux_history".to_string()
}

fn dx_public_dir(manifest_dir: &Path) -> PathBuf {
    let name = dioxus_application_name(manifest_dir);
    let expected = manifest_dir
        .join("target/dx")
        .join(&name)
        .join("debug/web/public");
    if expected.join("index.html").is_file() {
        return expected;
    }
    let dx_root = manifest_dir.join("target/dx");
    if let Ok(rd) = fs::read_dir(&dx_root) {
        for e in rd.flatten() {
            let cand = e.path().join("debug/web/public");
            if cand.join("index.html").is_file() {
                return cand;
            }
        }
    }
    expected
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let src_path = e.path();
        let dst_path = dst.join(e.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn build_history_web_dist_dx(manifest_dir: &Path) {
    let status = Command::new("dx")
        .args(["build", "--platform", "web"])
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "vmux_history: failed to run `dx build` ({e}). Install dioxus-cli or unset VMUX_HISTORY_USE_DX."
            )
        });
    if !status.success() {
        panic!(
            "vmux_history: `dx build --platform web` failed (VMUX_HISTORY_USE_DX).\n\
             The `dx` binary (crate `dioxus-cli`) must match `dioxus` in the repo root `Cargo.toml` ([workspace.dependencies]), e.g.:\n\
               cargo install dioxus-cli --version 0.7.4 --force\n\
             Or unset VMUX_HISTORY_USE_DX to use the default wasm-bindgen path."
        );
    }

    let public = dx_public_dir(manifest_dir);
    if !public.join("index.html").is_file() {
        panic!(
            "vmux_history: expected dx output at {} (index.html missing)",
            public.display()
        );
    }

    let dist = manifest_dir.join("dist");
    let web_dist = manifest_dir.join("web_dist");
    let _ = fs::remove_dir_all(&dist);
    let _ = fs::remove_dir_all(&web_dist);

    copy_dir_all(&public, &dist).unwrap_or_else(|e| {
        panic!(
            "vmux_history: failed to copy {} to {}: {e}",
            public.display(),
            dist.display()
        )
    });
    copy_dir_all(&dist, &web_dist).unwrap_or_else(|e| {
        panic!(
            "vmux_history: failed to copy {} to {}: {e}",
            dist.display(),
            web_dist.display()
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
