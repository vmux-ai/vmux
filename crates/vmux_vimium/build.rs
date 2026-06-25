use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("wasm32") {
        return;
    }
    if env::var("VMUX_VIMIUM_INNER_WASM").is_ok() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    let wasm_target_dir = out_dir.join("wasm-build");
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "build",
            "-p",
            "vmux_vimium",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--target-dir",
        ])
        .arg(&wasm_target_dir)
        .env("VMUX_VIMIUM_INNER_WASM", "1")
        .status()
        .expect("spawn cargo wasm build");
    assert!(status.success(), "vmux_vimium wasm build failed");

    let wasm_in = wasm_target_dir.join("wasm32-unknown-unknown/release/vmux_vimium.wasm");

    let bindgen_out = out_dir.join("bindgen");
    fs::create_dir_all(&bindgen_out).unwrap();
    let mut b = wasm_bindgen_cli_support::Bindgen::new();
    b.input_path(&wasm_in).typescript(false);
    b.no_modules(true).expect("no_modules mode");
    b.generate(&bindgen_out).expect("wasm-bindgen generate");

    let glue = fs::read_to_string(bindgen_out.join("vmux_vimium.js")).unwrap();
    let wasm_bytes = fs::read(bindgen_out.join("vmux_vimium_bg.wasm")).unwrap();
    let wasm_b64 = STANDARD.encode(&wasm_bytes);

    let template = fs::read_to_string(manifest_dir.join("src/preload.js.tmpl")).unwrap();
    let preload = template
        .replace("%%GLUE_JS%%", &glue)
        .replace("%%WASM_B64%%", &wasm_b64);
    fs::write(out_dir.join("vimium_preload.js"), preload).unwrap();
}
