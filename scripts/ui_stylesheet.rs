use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct Bundle;

impl Bundle {
    const SKIP_ENV: &str = "VMUX_SKIP_DX_BUILD";
    const SCANNED: &[&str] = &[
        "../vmux_browser/src",
        "../../feature/vmux_git/src",
        "src",
        "../../feature/vmux_service/src",
    ];

    pub(crate) fn build() {
        let skip_env = Self::SKIP_ENV;
        println!("cargo:rerun-if-env-changed={skip_env}");
        if std::env::var_os(Self::SKIP_ENV).is_some() {
            return;
        }
        let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());

        println!("cargo:rerun-if-changed=assets/index.css");
        println!("cargo:rerun-if-changed=assets/theme.css");
        println!("cargo:rerun-if-changed=assets/favicons");
        println!("cargo:rerun-if-changed=assets/agent-logos");
        println!("cargo:rerun-if-changed=assets/tool-logos");
        println!("cargo:rerun-if-changed=../../feature/vmux_terminal/assets/fonts");
        for scanned in Self::SCANNED {
            println!(
                "cargo:rerun-if-changed={}",
                manifest_dir.join(scanned).display()
            );
        }
        for source in Self::feature_crate_sources(&manifest_dir) {
            println!("cargo:rerun-if-changed={}", source.display());
        }

        let assets = manifest_dir.join("dist").join("assets");
        fs::create_dir_all(&assets).unwrap();
        Self::compile_index_css(&manifest_dir, &assets);
        Self::copy_file(
            &manifest_dir.join("assets/theme.css"),
            &assets.join("theme.css"),
        );
        Self::copy_dir(
            &manifest_dir.join("assets/favicons"),
            &assets.join("favicons"),
        );
        Self::copy_dir(
            &manifest_dir.join("assets/agent-logos"),
            &assets.join("agent-logos"),
        );
        Self::copy_dir(
            &manifest_dir.join("assets/tool-logos"),
            &assets.join("tool-logos"),
        );
        Self::copy_dir(
            &manifest_dir.join("../../feature/vmux_terminal/assets/fonts"),
            &assets.join("fonts"),
        );
        Self::write_bundle_stamp(&manifest_dir.join("dist"));
    }

    fn write_bundle_stamp(dist: &Path) {
        let stamp = dist.join(".bundle-stamp");
        let _ = fs::remove_file(&stamp);
        let mut files = Vec::new();
        Self::collect_files(dist, dist, &mut files);
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
                Self::collect_files(root, &path, out);
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

    fn feature_crate_sources(manifest_dir: &Path) -> Vec<PathBuf> {
        let bucket = manifest_dir.join("../../feature");
        let entries = fs::read_dir(&bucket).unwrap_or_else(|error| {
            panic!("cannot read crate bucket {}: {error}", bucket.display())
        });
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
                     {} to build without the stylesheet",
                    tailwind.display(), Self::SKIP_ENV
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
                Self::copy_file(&path, &destination.join(entry.file_name()));
            }
        }
    }
}
