use sha2::Digest;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::target_lock::TargetLock;
use crate::temporary::Temporary;

use super::Native;

impl Native {
    pub(super) fn cef_value(&self, key: &str) -> Result<String> {
        let table: toml::Table =
            fs::read_to_string(self.checkout.root.join("cef/manifest.toml"))?.parse()?;
        match table.get(key) {
            Some(toml::Value::String(value)) => Ok(value.clone()),
            Some(toml::Value::Integer(value)) => Ok(value.to_string()),
            _ => Err(format!("Missing CEF manifest value: {key}").into()),
        }
    }

    pub(super) fn framework(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.get(
                "CEF_FRAMEWORK_DIR",
                self.home()?
                    .join(".local/share/Chromium Embedded Framework.framework"),
            ),
        ))
    }

    pub fn validate_cef(&self, framework: &Path) -> Result<()> {
        let binary = framework.join("Chromium Embedded Framework");
        if fs::metadata(&binary)?.permissions().mode() & 0o111 == 0 {
            return Err("CEF framework binary is not executable".into());
        }
        let version = self.cef_value("cef_version")?;
        let numbers = version
            .split('.')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if numbers.len() != 3 {
            return Err("Invalid CEF version".into());
        }
        let expected = format!("{}.0.{}", numbers[0] * 10 + numbers[1], numbers[2]);
        let architectures = self.text("lipo", &[OsStr::new("-archs"), binary.as_os_str()])?;
        if !architectures.split_whitespace().any(|arch| arch == "arm64") {
            return Err("CEF framework is not ARM64".into());
        }
        let libraries = self.text("otool", &[OsStr::new("-L"), binary.as_os_str()])?;
        let actual = libraries
            .split("compatibility version ")
            .nth(1)
            .and_then(|value| value.split(',').next());
        if actual != Some(expected.as_str()) {
            return Err(format!("CEF version mismatch: expected {expected}").into());
        }
        let exports = self.text("nm", &[OsStr::new("-gU"), binary.as_os_str()])?;
        if !exports
            .lines()
            .any(|line| line.split_whitespace().last() == Some("_cef_set_os_crypt_keys"))
        {
            return Err("CEF does not export cef_set_os_crypt_keys".into());
        }
        Ok(())
    }

    pub fn install_cef(&self) -> Result<()> {
        let destination = self.framework()?;
        if cfg!(target_os = "macos") {
            if !cfg!(target_arch = "aarch64") {
                return Err("Custom CEF is published only for macOS ARM64".into());
            }
            if self.validate_cef(&destination).is_ok() {
                return Ok(());
            }
            let expected = self.cef_value("artifact_sha256")?;
            if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("CEF archive requires a pinned SHA-256 checksum".into());
            }
            let temporary = Temporary::create(&std::env::temp_dir(), "vmux-cef")?;
            let name = self.cef_value("artifact_name")?;
            let archive = temporary.path().join(&name);
            if let Some(source) = self.environment.get("VMUX_CEF_ARCHIVE") {
                self.copy(Path::new(source), &archive)?;
            } else {
                let default = format!(
                    "https://github.com/{}/releases/download/{}/{}",
                    self.cef_value("artifact_repository")?,
                    self.cef_value("artifact_tag")?,
                    name
                );
                self.download(
                    self.get("VMUX_CEF_URL", default)
                        .to_str()
                        .ok_or("Invalid CEF URL")?,
                    &archive,
                )?;
            }
            if self.checksum(&archive)? != expected {
                return Err("CEF archive checksum mismatch".into());
            }
            let extracted = temporary.path().join("extracted");
            fs::create_dir(&extracted)?;
            self.run_command(
                "tar",
                &[
                    OsStr::new("-xzf"),
                    archive.as_os_str(),
                    OsStr::new("-C"),
                    extracted.as_os_str(),
                ],
            )?;
            let framework = extracted.join("Chromium Embedded Framework.framework");
            self.validate_cef(&framework)?;
            fs::create_dir_all(
                destination
                    .parent()
                    .ok_or("Missing CEF destination parent")?,
            )?;
            if destination.exists() {
                fs::remove_dir_all(&destination)?;
            }
            fs::rename(framework, destination)?;
        } else {
            let lock: toml::Table =
                fs::read_to_string(self.checkout.root.join("Cargo.lock"))?.parse()?;
            let packages = lock
                .get("package")
                .and_then(toml::Value::as_array)
                .ok_or("Missing Cargo packages")?;
            let mut version = None;
            for package in packages {
                if package.get("name").and_then(toml::Value::as_str) == Some("cef") {
                    version = package.get("version").and_then(toml::Value::as_str);
                    break;
                }
            }
            let version = version.ok_or("Missing CEF version in lockfile")?;
            self.run_command(
                self.get("CARGO_BIN", "cargo"),
                &[
                    OsStr::new("install"),
                    OsStr::new(&format!("export-cef-dir@{version}")),
                    OsStr::new("--force"),
                ],
            )?;
            self.run_command(
                self.get("EXPORT_CEF_BIN", "export-cef-dir"),
                &[
                    OsStr::new("--force"),
                    destination.parent().unwrap().as_os_str(),
                ],
            )?;
        }
        Ok(())
    }

    pub fn helper(&mut self) -> Result<()> {
        self.cargo_environment()?;
        let installed = self
            .framework()?
            .join("Libraries/bevy_cef_debug_render_process");
        let cache = PathBuf::from(self.get(
            "VMUX_CEF_HELPER_CACHE",
            if cfg!(target_os = "macos") {
                self.home()?
                    .join("Library/Caches/Vmux/cef-debug-render-process")
            } else {
                PathBuf::from(self.get("XDG_CACHE_HOME", self.home()?.join(".cache")))
                    .join("vmux/cef-debug-render-process")
            },
        ));
        fs::create_dir_all(&cache)?;
        let _lock = TargetLock::acquire(&self.checkout.root, &cache)?;
        let fingerprint = self.helper_fingerprint()?;
        let stamp = installed.with_file_name(".vmux-bevy_cef_debug_render_process.fingerprint");
        if installed.is_file()
            && fs::read_to_string(&stamp).is_ok_and(|value| value.trim() == fingerprint)
        {
            return Ok(());
        }
        let cached = cache
            .join(&fingerprint)
            .join("bevy_cef_debug_render_process");
        if !cached.is_file() {
            self.cargo(
                &[
                    "build",
                    "-p",
                    "bevy_cef_debug_render_process",
                    "--features",
                    "debug",
                ]
                .map(Into::into),
            )?;
            let built = self
                .checkout
                .directory("debug")
                .join("bevy_cef_debug_render_process");
            if fs::metadata(&built)?.permissions().mode() & 0o111 == 0 {
                return Err("CEF helper build produced no executable".into());
            }
            let temporary = cached.with_extension("tmp");
            self.copy(&built, &temporary)?;
            fs::rename(temporary, &cached)?;
        }
        let temporary = installed.with_extension("tmp");
        self.copy(&cached, &temporary)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
        fs::rename(temporary, &installed)?;
        let temporary = stamp.with_extension("tmp");
        fs::write(&temporary, format!("{fingerprint}\n"))?;
        fs::rename(temporary, stamp)?;
        Ok(())
    }

    fn helper_fingerprint(&self) -> Result<String> {
        let mut paths = vec![
            self.checkout.root.join("Cargo.toml"),
            self.checkout.root.join("Cargo.lock"),
        ];
        for name in [
            "bevy_cef_core-0.5.2",
            "bevy_cef_debug_render_process-0.5.2",
            "bevy_window-0.19.0",
        ] {
            Self::files(&self.checkout.root.join("patches").join(name), &mut paths)?;
        }
        paths.sort();
        let mut bytes = Vec::new();
        bytes.extend(
            self.text(self.get("RUSTC", "rustc"), &[OsStr::new("-vV")])?
                .bytes(),
        );
        for path in paths {
            bytes.extend(self.checksum(&path)?.bytes());
        }
        for key in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_TARGET",
            "MACOSX_DEPLOYMENT_TARGET",
            "VMUX_CEF_SDK_CACHE",
            "RUSTC_WRAPPER",
        ] {
            bytes.extend(self.get(key, "").as_encoded_bytes());
        }
        Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
    }
}
