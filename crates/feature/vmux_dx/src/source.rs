use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub struct Source;

impl Source {
    pub fn paths(root: &Path, canceled: &impl Fn() -> bool) -> std::io::Result<Vec<PathBuf>> {
        if canceled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "CLI bootstrap canceled",
            ));
        };
        let mut paths = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
        let mut directories = 0usize;
        for name in [
            "vmux_dx",
            "vmux_cli",
            "vmux_ecs",
            "vmux_api",
            "vmux_channel",
            "vmux_job",
            "vmux_store",
            "vmux_watch",
            "vmux_task",
            "vmux_time",
            "vmux_macro",
        ] {
            let group = match name {
                "vmux_dx" => "feature",
                "vmux_cli" => "app",
                _ => "util",
            };
            let directory = root.join("crates").join(group).join(name);
            if !directory.is_dir() {
                continue;
            }
            paths.push(directory.join("Cargo.toml"));
            Self::files(
                &directory.join("src"),
                &mut paths,
                0,
                &mut directories,
                canceled,
            )?;
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

    fn files(
        path: &Path,
        paths: &mut Vec<PathBuf>,
        depth: usize,
        directories: &mut usize,
        canceled: &impl Fn() -> bool,
    ) -> std::io::Result<()> {
        crate::source_io::SourceFsIo::walk_source_files(path, paths, depth, directories, canceled)
    }

    pub fn key(root: &Path, canceled: impl Fn() -> bool) -> std::io::Result<String> {
        let mut hash = Sha256::new();
        let mut total = 0usize;
        for path in Self::paths(root, &canceled)? {
            if canceled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "CLI bootstrap canceled",
                ));
            }
            hash.update(
                path.strip_prefix(root)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            hash.update([0]);
            let file = crate::source_io::SourceFsIo::open_regular_nofollow(&path)?;
            let mut file = file.take(16 * 1024 * 1024 + 1);
            let mut size = 0usize;
            let mut bytes = [0u8; 65536];
            loop {
                if canceled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "CLI bootstrap canceled",
                    ));
                }
                let count = file.read(&mut bytes)?;
                if count == 0 {
                    break;
                }
                size += count;
                total += count;
                if size > 16 * 1024 * 1024 || total > 64 * 1024 * 1024 {
                    return Err(std::io::Error::other(
                        "CLI compiler input exceeds its byte bound",
                    ));
                }
                hash.update(&bytes[..count]);
            }
            hash.update([0]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
}
