use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::execution::Execution;
use crate::signals::Signals;
use crate::target_lock::TargetLock;
use crate::temporary::Temporary;

use super::Native;

impl Native {
    pub(super) fn files(path: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | ".worktrees" | ".build-tools" | "target")
            ) {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                Self::files(&entry.path(), output)?;
            } else if kind.is_file() {
                output.push(entry.path());
            }
        }
        Ok(())
    }

    pub(super) fn seed_key(&self) -> Result<String> {
        let mut paths = Vec::new();
        Self::files(&self.checkout.root, &mut paths)?;
        paths.retain(|path| {
            matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("Cargo.toml" | "Cargo.lock" | "rust-toolchain" | "rust-toolchain.toml")
            ) || path
                .parent()
                .is_some_and(|parent| parent.file_name() == Some(OsStr::new(".cargo")))
                && matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("config" | "config.toml")
                )
        });
        paths.sort();
        let cargo = self.text(self.get("CARGO_BIN", "cargo"), &[OsStr::new("-V")])?;
        let rustc = self.text(self.get("RUSTC", "rustc"), &[OsStr::new("-vV")])?;
        let mut input = format!("cargo={}\nrustc={}\n", cargo.trim_end(), rustc.trim_end());
        if self.environment.get("VMUX_DX_PLATFORM") == Some(OsStr::new("web")) {
            input.push_str("vmux-dx-platform=web\n");
        }
        for (label, key) in [
            ("rustc-wrapper", "RUSTC_WRAPPER"),
            ("rustc-workspace-wrapper", "RUSTC_WORKSPACE_WRAPPER"),
            ("cargo-incremental", "CARGO_INCREMENTAL"),
            ("rustflags", "RUSTFLAGS"),
            ("cargo-encoded-rustflags", "CARGO_ENCODED_RUSTFLAGS"),
            ("cargo-build-target", "CARGO_BUILD_TARGET"),
            ("macosx-deployment-target", "MACOSX_DEPLOYMENT_TARGET"),
        ] {
            input.push_str(&format!(
                "{label}={}\n",
                self.get(key, "").to_string_lossy()
            ));
        }
        for path in &paths {
            input.push_str(&format!(
                "file={}\n",
                path.strip_prefix(&self.checkout.root)?.display()
            ));
        }
        if !paths.is_empty() {
            let mut command = self.command("git");
            command.arg("hash-object").args(paths);
            input.push_str(std::str::from_utf8(
                &Execution::new(command).output(None)?.stdout,
            )?);
        }
        let mut command = self.command("git");
        command.args(["hash-object", "--stdin"]);
        Ok(String::from_utf8(
            Execution::new(command)
                .output(Some(input.as_bytes()))?
                .stdout,
        )?
        .trim()
        .into())
    }

    pub fn record_seed(&self) -> Result<()> {
        let key = self.seed_key()?;
        let marker = self.checkout.target.join(".vmux-seed");
        if marker.is_dir() {
            fs::remove_dir_all(&marker)?;
        }
        fs::create_dir_all(&marker)?;
        fs::write(marker.join(key), "")?;
        Ok(())
    }

    pub fn seed(&mut self) -> Result<()> {
        if self.checkout.target.exists() || self.environment.get("CI") == Some(OsStr::new("true")) {
            return Ok(());
        }
        self.cargo_environment()?;
        let key = self.seed_key()?;
        let source = if let Some(source) = self.environment.get("VMUX_TARGET_SEED") {
            Some(PathBuf::from(source))
        } else {
            let text = self.text(
                "git",
                &[
                    OsStr::new("worktree"),
                    OsStr::new("list"),
                    OsStr::new("--porcelain"),
                ],
            )?;
            let mut newest = None;
            for line in text.lines() {
                let Some(path) = line.strip_prefix("worktree ") else {
                    continue;
                };
                let relative = self
                    .checkout
                    .target
                    .strip_prefix(&self.checkout.root)
                    .unwrap_or(Path::new("target"));
                let target = Path::new(path).join(relative);
                let marker = target.join(".vmux-seed").join(&key);
                if target == self.checkout.target
                    || fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.is_symlink())
                {
                    continue;
                }
                let Ok(metadata) = fs::metadata(marker) else {
                    continue;
                };
                let modified = metadata.modified()?;
                if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
                    newest = Some((modified, target));
                }
            }
            newest.map(|(_, target)| target)
        };
        let Some(source) = source else { return Ok(()) };
        if !source.is_dir() || fs::symlink_metadata(&source)?.is_symlink() {
            return Ok(());
        }
        let source = source.canonicalize()?;
        if source == self.checkout.target {
            return Ok(());
        }
        let Some(_source_lock) = TargetLock::try_acquire(&self.checkout.root, &source)? else {
            println!("Compatible target seed is busy; building without seed");
            return Ok(());
        };
        if !source.join(".vmux-seed").join(&key).is_file() {
            return Ok(());
        }
        let staging = Temporary::create(
            self.checkout
                .target
                .parent()
                .ok_or("Missing target parent")?,
            ".vmux-target-seed",
        )?;
        let clone = staging.path().join("target");
        let mut command = self.command("cp");
        if cfg!(target_os = "macos") {
            command.arg("-cR");
        } else {
            command.args(["--reflink=always", "-a"]);
        }
        command.arg(&source).arg(&clone);
        Execution::new(command).run()?;
        self.relocate(&clone, &source, &self.checkout.target)?;
        Signals::check()?;
        if self.checkout.target.exists() {
            return Err("Target appeared during seed".into());
        }
        fs::rename(clone, &self.checkout.target)?;
        println!(
            "Seeded target: {} -> {}",
            source.display(),
            self.checkout.target.display()
        );
        Ok(())
    }

    pub(super) fn relocate(&self, staging: &Path, source: &Path, destination: &Path) -> Result<()> {
        let mut files = Vec::new();
        Self::files(staging, &mut files)?;
        let mut prefixes = vec![source.to_string_lossy().into_owned()];
        for path in &files {
            if path.file_name() != Some(OsStr::new("CMakeCache.txt"))
                || !path.to_string_lossy().contains("cef-dll-sys-")
            {
                continue;
            }
            let text = fs::read_to_string(path)?;
            let suffix = path
                .parent()
                .unwrap()
                .strip_prefix(staging)?
                .to_string_lossy();
            for line in text.lines() {
                if let Some(directory) = line.strip_prefix("CMAKE_CACHEFILE_DIR:INTERNAL=")
                    && let Some(prefix) = directory.strip_suffix(&format!("/{suffix}"))
                    && !prefixes.iter().any(|existing| existing == prefix)
                {
                    prefixes.push(prefix.into());
                }
            }
        }
        for path in files {
            Signals::check()?;
            let name = path.to_string_lossy();
            if path
                .components()
                .any(|part| part.as_os_str() == "incremental")
            {
                continue;
            }
            if !name.contains("cef-dll-sys-") && !name.contains("cef_dll_sys-") {
                continue;
            }
            if !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("json" | "cmake" | "d" | "ninja" | "txt" | "yaml")
            ) && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("output" | "root-output" | "stderr")
            ) {
                continue;
            }
            let Ok(mut text) = fs::read_to_string(&path) else {
                continue;
            };
            for prefix in &prefixes {
                text = text.replace(prefix, &destination.to_string_lossy());
            }
            fs::write(path, text)?;
        }
        Self::remove_incremental(staging)
    }

    fn remove_incremental(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if entry.file_name() == "incremental" {
                fs::remove_dir_all(entry.path())?;
            } else {
                Self::remove_incremental(&entry.path())?;
            }
        }
        Ok(())
    }
}
