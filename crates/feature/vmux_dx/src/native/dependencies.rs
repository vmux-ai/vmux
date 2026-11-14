use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::Result;
use crate::execution::Execution;

use super::Native;

impl Native {
    fn versions(&self) -> Result<toml::Table> {
        let source = fs::read_to_string(self.checkout.root.join("tool-versions.env"))?;
        let mut values = toml::Table::new();
        for line in source.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            values.insert(
                key.into(),
                toml::Value::String(value.trim_matches('"').into()),
            );
        }
        Ok(values)
    }

    fn version(&self, key: &str) -> Result<String> {
        self.versions()?
            .get(key)
            .and_then(toml::Value::as_str)
            .map(String::from)
            .ok_or_else(|| format!("Missing tool version: {key}").into())
    }

    fn installed(&self, program: &OsStr, version: &str) -> bool {
        self.installed_with(program, version, "--version")
    }

    fn installed_with(&self, program: &OsStr, version: &str, option: &str) -> bool {
        let mut command = self.command(program);
        command.arg(option);
        let output = Execution::new(command).output_any(None);
        match output {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .chain(String::from_utf8_lossy(&output.stderr).split_whitespace())
                .any(|word| word.trim_start_matches('v') == version),
            _ => false,
        }
    }

    pub(super) fn download(&self, url: &str, path: &std::path::Path) -> Result<()> {
        let mut command = self.command("curl");
        command
            .args([
                "--retry",
                "5",
                "--retry-delay",
                "2",
                "--retry-max-time",
                "120",
                "-fsSL",
                "-o",
            ])
            .arg(path)
            .arg(url);
        Execution::new(command).run()
    }

    pub fn ensure_native(&mut self) -> Result<()> {
        self.text(self.get("CARGO_BIN", "cargo"), &[OsStr::new("--version")])?;
        self.text(self.get("RUSTUP_BIN", "rustup"), &[OsStr::new("--version")])?;
        if cfg!(target_os = "macos") {
            self.install_cef()?;
        } else if !self
            .home()?
            .join(".local/share/Chromium Embedded Framework.framework")
            .is_dir()
        {
            return Err("CEF framework missing; run vmux internal install-cef".into());
        }
        Ok(())
    }

    pub fn package_tools(&mut self) -> Result<()> {
        let packager = self.get("CARGO_PACKAGER_BIN", "cargo-packager");
        let version = self.version("CARGO_PACKAGER_VERSION")?;
        if !self.installed(&packager, &version) {
            let path = self.checkout.root.join("patches/cargo-packager");
            let mut command = self.command(self.get("CARGO_BIN", "cargo"));
            command
                .args(["install", "--path"])
                .arg(path)
                .args(["--locked", "--force"]);
            Execution::new(command).run()?;
        }
        let bundle = self.get("BEVY_CEF_BUNDLE_APP_BIN", "bevy_cef_bundle_app");
        let version = self.version("BEVY_CEF_BUNDLE_APP_VERSION")?;
        if !self.installed(&bundle, &version) {
            let mut command = self.command(self.get("CARGO_BIN", "cargo"));
            command
                .args(["install", "bevy_cef_bundle_app", "--locked", "--version"])
                .arg(version);
            Execution::new(command).run()?;
        }
        Ok(())
    }

    pub fn dioxus(&mut self, platform: &str) -> Result<()> {
        self.install_dioxus()?;
        let targets: &[&str] = match platform {
            "ios" => &[
                "aarch64-apple-ios",
                "aarch64-apple-ios-sim",
                "x86_64-apple-ios",
            ],
            "android" => &[
                "aarch64-linux-android",
                "armv7-linux-androideabi",
                "i686-linux-android",
                "x86_64-linux-android",
            ],
            "web" => &["wasm32-unknown-unknown"],
            _ => return Err("Unsupported Dioxus platform".into()),
        };
        let installed = self.text(
            self.get("RUSTUP_BIN", "rustup"),
            &[
                OsStr::new("target"),
                OsStr::new("list"),
                OsStr::new("--installed"),
            ],
        )?;
        for target in targets {
            if !installed.lines().any(|line| line == *target) {
                self.run_command(
                    self.get("RUSTUP_BIN", "rustup"),
                    &[OsStr::new("target"), OsStr::new("add"), OsStr::new(target)],
                )?;
            }
        }
        if platform == "android" {
            self.environment.required("ANDROID_HOME")?;
            self.environment.required("ANDROID_NDK_HOME")?;
            self.text("java", &[OsStr::new("--version")])?;
        }
        Ok(())
    }

    pub fn install_dioxus(&mut self) -> Result<()> {
        let version = self.version("DIOXUS_CLI_VERSION")?;
        let binary = self.get("DX_BIN", "dx");
        if self.installed(&binary, &version) {
            return Ok(());
        }
        let directory = PathBuf::from(self.get(
            "DIOXUS_CLI_INSTALL_DIR",
            self.tool_cache()?.join("dioxus").join(&version),
        ));
        let destination = directory.join("dx");
        if self.installed(destination.as_os_str(), &version) {
            self.environment.set("DX_BIN", destination.as_os_str());
            self.expose_tool(&destination)?;
            return Ok(());
        }
        let asset = match (cfg!(target_os = "macos"), cfg!(target_arch = "aarch64")) {
            (true, true) => "dx-aarch64-apple-darwin.tar.gz",
            (true, false) => "dx-x86_64-apple-darwin.tar.gz",
            (false, true) => "dx-aarch64-unknown-linux-gnu.tar.gz",
            (false, false) => "dx-x86_64-unknown-linux-gnu.tar.gz",
        };
        let temporary = crate::temporary::Temporary::create(&std::env::temp_dir(), "vmux-dioxus")?;
        let archive = temporary.path().join(asset);
        let checksums = temporary.path().join("checksums");
        let base = format!("https://github.com/DioxusLabs/dioxus/releases/download/v{version}");
        self.download(&format!("{base}/{asset}"), &archive)?;
        self.download(
            &format!("{base}/{}.sha256", asset.trim_end_matches(".tar.gz")),
            &checksums,
        )?;
        let source = fs::read_to_string(checksums)?;
        let expected = source
            .lines()
            .find_map(|line| {
                let (hash, name) = line.split_once("  ")?;
                if name == asset { Some(hash) } else { None }
            })
            .ok_or("Published Dioxus checksum is missing")?;
        if self.checksum(&archive)? != expected {
            return Err("Dioxus CLI checksum mismatch".into());
        }
        self.run_command(
            "tar",
            &[
                OsStr::new("-xzf"),
                archive.as_os_str(),
                OsStr::new("-C"),
                temporary.path().as_os_str(),
            ],
        )?;
        self.install_binary(
            &temporary.path().join("dx"),
            &destination,
            &version,
            "--version",
        )?;
        if !self.installed(destination.as_os_str(), &version) {
            return Err("Installed Dioxus version does not match the pin".into());
        }
        self.environment.set("DX_BIN", destination.as_os_str());
        self.expose_tool(&destination)?;
        Ok(())
    }

    pub fn binaryen(&self) -> Result<()> {
        let version = self.version("BINARYEN_VERSION")?;
        let base = if cfg!(target_os = "macos") {
            self.home()?.join(".dx")
        } else {
            PathBuf::from(self.get("XDG_DATA_HOME", self.home()?.join(".local/share"))).join(".dx")
        };
        let directory = PathBuf::from(self.get("DX_HOME", base))
            .join("tools")
            .join(format!("binaryen-{version}"));
        if self.installed(directory.join("bin/wasm-opt").as_os_str(), &version) {
            return Ok(());
        }
        let architecture = match (cfg!(target_os = "macos"), cfg!(target_arch = "aarch64")) {
            (true, true) => "arm64-macos",
            (true, false) => "x86_64-macos",
            (false, true) => "aarch64-linux",
            (false, false) => "x86_64-linux",
        };
        let temporary =
            crate::temporary::Temporary::create(&std::env::temp_dir(), "vmux-binaryen")?;
        let archive = temporary.path().join("binaryen.tar.gz");
        self.download(&format!("https://github.com/WebAssembly/binaryen/releases/download/version_{version}/binaryen-version_{version}-{architecture}.tar.gz"), &archive)?;
        fs::create_dir_all(&directory)?;
        self.run_command(
            "tar",
            &[
                OsStr::new("-xzf"),
                archive.as_os_str(),
                OsStr::new("--strip-components"),
                OsStr::new("1"),
                OsStr::new("-C"),
                directory.as_os_str(),
            ],
        )?;
        Ok(())
    }

    pub fn tailwind(&mut self) -> Result<()> {
        let version = self.version("TAILWINDCSS_VERSION")?;
        let binary = self.get("TAILWINDCSS_BIN", "tailwindcss");
        if self.installed_with(&binary, &version, "--help") {
            return Ok(());
        }
        let directory = PathBuf::from(self.get(
            "TAILWINDCSS_INSTALL_DIR",
            self.tool_cache()?.join("tailwind").join(&version),
        ));
        let destination = directory.join("tailwindcss");
        if self.installed_with(destination.as_os_str(), &version, "--help") {
            self.environment
                .set("TAILWINDCSS_BIN", destination.as_os_str());
            self.expose_tool(&destination)?;
            return Ok(());
        }
        let platform = match (cfg!(target_os = "macos"), cfg!(target_arch = "aarch64")) {
            (true, true) => "macos-arm64",
            (true, false) => "macos-x64",
            (false, true) => "linux-arm64",
            (false, false) => "linux-x64",
        };
        let temporary =
            crate::temporary::Temporary::create(&std::env::temp_dir(), "vmux-tailwind")?;
        let downloaded = temporary.path().join("tailwindcss");
        self.download(&format!("https://github.com/tailwindlabs/tailwindcss/releases/download/v{version}/tailwindcss-{platform}"), &downloaded)?;
        self.install_binary(&downloaded, &destination, &version, "--help")?;
        self.environment
            .set("TAILWINDCSS_BIN", destination.as_os_str());
        self.expose_tool(&destination)?;
        Ok(())
    }

    fn install_binary(
        &self,
        source: &std::path::Path,
        destination: &std::path::Path,
        version: &str,
        probe: &str,
    ) -> Result<()> {
        let directory = destination
            .parent()
            .ok_or("Missing binary destination parent")?;
        fs::create_dir_all(directory)?;
        let temporary = crate::temporary::Temporary::create(directory, ".vmux-tool")?;
        let binary = temporary.path().join("binary");
        self.copy(source, &binary)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
        if !self.installed_with(binary.as_os_str(), version, probe) {
            return Err("Downloaded tool version does not match the pin".into());
        }
        fs::rename(binary, destination)?;
        if cfg!(target_os = "macos") {
            let _ = self
                .command("xattr")
                .args(["-d", "com.apple.quarantine"])
                .arg(destination)
                .status();
        }
        Ok(())
    }

    fn tool_cache(&self) -> Result<PathBuf> {
        Ok(if cfg!(target_os = "macos") {
            self.home()?.join("Library/Caches/Vmux/tools")
        } else {
            PathBuf::from(self.get("XDG_CACHE_HOME", self.home()?.join(".cache")))
                .join("vmux/tools")
        })
    }

    fn expose_tool(&self, executable: &std::path::Path) -> Result<()> {
        if let Some(path) = self.environment.get("GITHUB_PATH") {
            let parent = executable.parent().ok_or("Missing tool directory")?;
            let value = parent.to_string_lossy();
            if value.contains(['\r', '\n']) {
                return Err("CI tool path must be a single line".into());
            }
            writeln!(fs::OpenOptions::new().append(true).open(path)?, "{value}")?;
        }
        Ok(())
    }

    pub fn tool_path(&self, key: &str, default: &str) -> OsString {
        self.get(key, default)
    }

    pub fn doctor(&self) -> Result<()> {
        let mut missing = false;
        for (key, name) in [
            ("CARGO_BIN", "cargo"),
            ("RUSTUP_BIN", "rustup"),
            ("DX_BIN", "dx"),
            ("CARGO_PACKAGER_BIN", "cargo-packager"),
            ("BEVY_CEF_BUNDLE_APP_BIN", "bevy_cef_bundle_app"),
        ] {
            let binary = self.get(key, name);
            let mut command = self.command(&binary);
            command.arg("--version");
            match Execution::new(command).output_any(None) {
                Ok(output) if output.status.success() => println!(
                    "{}: {}",
                    name,
                    String::from_utf8_lossy(&output.stdout).trim()
                ),
                _ => {
                    eprintln!("{name}: missing");
                    missing = true;
                }
            }
        }
        if cfg!(target_os = "macos") {
            match self.validate_cef(&self.framework()?) {
                Ok(()) => println!("CEF: verified"),
                Err(error) => {
                    eprintln!("CEF: {error}");
                    missing = true;
                }
            }
        }
        if missing {
            return Err("Development tools are incomplete".into());
        }
        Ok(())
    }

    pub(super) fn cargo_environment(&mut self) -> Result<()> {
        self.environment
            .set("VMUX_TARGET_LOCK_TARGET", self.checkout.target.as_os_str());
        self.environment
            .set("VMUX_TARGET_LOCK_OWNER_PID", std::process::id().to_string());
        self.environment.set(
            "VMUX_BUILD_STORAGE_TARGET",
            self.checkout.target.as_os_str(),
        );
        self.environment
            .set("VMUX_BUILD_STORAGE_ROOT", self.checkout.root.as_os_str());
        self.environment
            .set("VMUX_DX_BIN", std::env::current_exe()?.as_os_str());
        if let Some(binary) = self.environment.get("TAILWINDCSS_BIN") {
            self.environment.set("TAILWINDCSS", binary.to_owned());
        }
        let cache = match self.environment.get("VMUX_CEF_SDK_CACHE") {
            Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
            _ if self.environment.get("CI") != Some(OsStr::new("true")) => {
                Some(if cfg!(target_os = "macos") {
                    self.home()?.join("Library/Caches/Vmux/cef-sdk")
                } else {
                    PathBuf::from(self.get("XDG_CACHE_HOME", self.home()?.join(".cache")))
                        .join("vmux/cef-sdk")
                })
            }
            _ => None,
        };
        self.environment.remove("CEF_PATH");
        if let Some(cache) = cache {
            fs::create_dir_all(&cache)?;
            self.environment.set("CEF_PATH", cache.as_os_str());
        }
        let mut probe = self.command("sccache");
        probe.arg("--version");
        if self.environment.get("VMUX_DISABLE_SCCACHE") != Some(OsStr::new("1"))
            && Execution::new(probe)
                .output_any(None)
                .is_ok_and(|output| output.status.success())
        {
            let rustc = self.get("RUSTC", "rustc");
            let mut probe = self.command("sccache");
            probe.arg(&rustc).arg("-vV");
            if Execution::new(probe)
                .output_any(None)
                .is_ok_and(|output| output.status.success())
            {
                for name in ["CMAKE_C_COMPILER_LAUNCHER", "CMAKE_CXX_COMPILER_LAUNCHER"] {
                    if !self.environment.present(name) {
                        self.environment.set(name, "sccache");
                    }
                }
                if self.environment.get("CI") != Some(OsStr::new("true"))
                    && !self.environment.present("RUSTC_WRAPPER")
                {
                    self.environment.set("RUSTC_WRAPPER", "sccache");
                    if !self.environment.present("CARGO_INCREMENTAL") {
                        self.environment.set("CARGO_INCREMENTAL", "0");
                    }
                }
            }
        }
        if self.environment.get("VMUX_BUILD_PROFILE") == Some(OsStr::new("dev")) {
            self.environment.set("CARGO_INCREMENTAL", "1");
        }
        Ok(())
    }
}
