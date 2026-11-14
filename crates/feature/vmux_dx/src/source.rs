use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub struct Source;

impl Source {
    pub fn paths(root: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut paths = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
        for name in [
            "vmux_dx",
            "vmux_cli",
            "vmux_ecs",
            "vmux_api",
            "vmux_task",
            "vmux_macro",
        ] {
            let group = match name {
                "vmux_dx" => "feature",
                "vmux_cli" => "app",
                _ => "util",
            };
            let directory = root.join("crates").join(group).join(name);
            paths.push(directory.join("Cargo.toml"));
            Self::files(&directory.join("src"), &mut paths)?;
            let build = directory.join("build.rs");
            if build.is_file() {
                paths.push(build);
            }
        }
        for name in [
            "rust-toolchain.toml",
            "rust-toolchain",
            "tool-versions.env",
            "crates/build_cfg_git.rs",
            "crates/build_git_env.rs",
            "crates/build_platform_cfg.rs",
        ] {
            let path = root.join(name);
            if path.is_file() {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn files(path: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                Self::files(&entry.path(), paths)?;
            } else if kind.is_file()
                && matches!(
                    entry.path().extension().and_then(|value| value.to_str()),
                    Some("rs" | "ron")
                )
            {
                paths.push(entry.path());
            }
        }
        Ok(())
    }

    pub fn key(root: &Path) -> std::io::Result<String> {
        let mut hash = Sha256::new();
        for path in Self::paths(root)? {
            hash.update(
                path.strip_prefix(root)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            hash.update([0]);
            hash.update(fs::read(path)?);
            hash.update([0]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
}
