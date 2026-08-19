//! Generates the Fluent catalogue index, and builds the stylesheet bundle every page links.
//!
//! The bundle used to fall out of a `dx build --platform web`, because the pages were wasm. They
//! are not any more, and Tailwind never read that build's output anyway — every `@source` in
//! `assets/index.css` names a Rust source directory. So this runs the CLI directly and copies the
//! two static things beside it.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../build_platform_cfg.rs"]
mod build_platform_cfg;

/// Set by CI, which has no Tailwind CLI and does not package the app.
const SKIP_ENV: &str = "VMUX_SKIP_DX_BUILD";

/// Directories whose Rust source Tailwind scans for class names. Must agree with the `@source`
/// list in `assets/index.css`: a directory missing here does not fail the build, it just stops
/// the stylesheet being rebuilt when that page's classes change.
const SCANNED: &[&str] = &[
    "../vmux_browser/src",
    "../vmux_ui/src",
    "../host/vmux_service/src",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    build_platform_cfg::emit();
    generate_i18n_catalogs();
    build_stylesheet_bundle();
}

fn generate_i18n_catalogs() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let locales_dir = manifest_dir.join("locales");
    println!("cargo:rerun-if-changed={}", locales_dir.display());
    let mut locales = fs::read_dir(&locales_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
        .filter_map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    locales.sort();
    assert!(locales.iter().any(|locale| locale == "en-US"));

    let catalogs = locales
        .iter()
        .map(|locale| {
            format!(
                "    (\"{locale}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/locales/{locale}.ftl\"))),\n"
            )
        })
        .collect::<String>();
    let available = locales
        .iter()
        .map(|locale| format!("    \"{locale}\",\n"))
        .collect::<String>();
    let generated = format!(
        "pub const EMBEDDED_CATALOGS: &[(&str, &str)] = &[\n{catalogs}];\n\npub const AVAILABLE_LOCALES: &[&str] = &[\n{available}];\n"
    );
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("i18n_catalogs.rs");
    if fs::read_to_string(&output).ok().as_deref() != Some(generated.as_str()) {
        fs::write(output, generated).unwrap();
    }
}

fn build_stylesheet_bundle() {
    println!("cargo:rerun-if-env-changed={SKIP_ENV}");
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());

    println!("cargo:rerun-if-changed=assets/index.css");
    println!("cargo:rerun-if-changed=assets/theme.css");
    println!("cargo:rerun-if-changed=../page/vmux_terminal/assets/fonts");
    for scanned in SCANNED {
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join(scanned).display()
        );
    }
    for page in page_crate_sources(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", page.display());
    }

    if std::env::var_os(SKIP_ENV).is_some() {
        return;
    }

    let assets = manifest_dir.join("dist").join("assets");
    fs::create_dir_all(&assets).unwrap();
    compile_index_css(&manifest_dir, &assets);
    copy_file(
        &manifest_dir.join("assets/theme.css"),
        &assets.join("theme.css"),
    );
    // The terminal owns the font it renders in; every other page just inherits it.
    copy_dir(
        &manifest_dir.join("../page/vmux_terminal/assets/fonts"),
        &assets.join("fonts"),
    );
    write_bundle_stamp(&manifest_dir.join("dist"));
}

/// A SHA-256 manifest of the bundle, checked again after it is copied into the `.app`, so a
/// partial or corrupted copy fails packaging instead of shipping a page with no styles.
fn write_bundle_stamp(dist: &Path) {
    let stamp = dist.join(".bundle-stamp");
    let _ = fs::remove_file(&stamp);
    let mut files = Vec::new();
    collect_files(dist, dist, &mut files);
    files.sort();
    let mut manifest = String::new();
    for relative in files {
        let digest = Sha256::digest(fs::read(dist.join(&relative)).expect("bundle file"));
        manifest.push_str(&format!("{digest:x}  {relative}\n"));
    }
    fs::write(&stamp, manifest).expect("bundle stamp");
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("bundle directory").flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .expect("inside the bundle")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

/// Enumerating the bucket rather than listing its crates is what keeps a page added later from
/// being silently unscanned.
fn page_crate_sources(manifest_dir: &Path) -> Vec<PathBuf> {
    let bucket = manifest_dir.join("../page");
    let entries = fs::read_dir(&bucket)
        .unwrap_or_else(|error| panic!("cannot read crate bucket {}: {error}", bucket.display()));
    let mut sources = Vec::new();
    for entry in entries {
        let path = entry.expect("bucket entry").path().join("src");
        if path.is_dir() {
            sources.push(path);
        }
    }
    sources.sort();
    sources
}

fn compile_index_css(manifest_dir: &Path, assets: &Path) {
    let tailwind = std::env::var_os("TAILWINDCSS")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("tailwindcss"));
    let status = Command::new(&tailwind)
        .args(["-i", "assets/index.css", "-o"])
        .arg(assets.join("index.css"))
        .arg("--minify")
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "{} not runnable ({error}) — install the Tailwind v4 CLI, set TAILWINDCSS, or set \
                 {SKIP_ENV} to build without the stylesheet",
                tailwind.display()
            )
        });
    assert!(status.success(), "tailwindcss exited with {status}");
}

fn copy_file(source: &Path, destination: &Path) {
    fs::copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "cannot copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    });
}

fn copy_dir(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).expect("asset directory").flatten() {
        let path = entry.path();
        if path.is_file() {
            copy_file(&path, &destination.join(entry.file_name()));
        }
    }
}
