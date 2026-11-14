use base64::{Engine, engine::general_purpose::STANDARD};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;

use super::Native;
use super::package::Publication;
use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

impl Native {
    pub fn cli_compile(&self) -> Result<()> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err("Standalone CLI release packaging currently targets macOS aarch64".into());
        }
        self.environment.required("VMUX_UPDATE_PUBLIC_KEY")?;
        let mut command = self.command(
            self.environment
                .get("CARGO_BIN")
                .unwrap_or(OsStr::new("cargo")),
        );
        command
            .args([
                "build",
                "--locked",
                "--release",
                "-p",
                "vmux_cli",
                "--no-default-features",
                "--features",
                "dx",
                "--target",
                "aarch64-apple-darwin",
                "--target-dir",
            ])
            .arg(&self.checkout.target)
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("RUSTFLAGS");
        Execution::new(command).run()
    }

    pub fn cli_package(&self, preview: bool) -> Result<Vec<std::path::PathBuf>> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err("Standalone CLI release packaging currently targets macOS aarch64".into());
        }
        let version = self.workspace_version()?;
        let binary = self
            .checkout
            .target
            .join("aarch64-apple-darwin/release/vmux");
        let mut probe = self.command(&binary);
        probe.arg("--version").env("VMUX_CLI_UPDATE_HANDOFF", "1");
        let output = Execution::new(probe).output(None)?;
        if std::str::from_utf8(&output.stdout)?.trim() != format!("vmux {version}") {
            return Err("Standalone release CLI does not match the workspace version".into());
        }
        let name = if preview {
            "vmux-cli-nightly-aarch64-apple-darwin.tar.gz".into()
        } else {
            format!("vmux-cli-v{version}-aarch64-apple-darwin.tar.gz")
        };
        let directory = self.checkout.directory("release");
        fs::create_dir_all(&directory)?;
        let temporary = Temporary::create(&directory, ".cli-release")?;
        let archive = temporary.path().join(&name);
        let output = fs::File::create(&archive)?;
        let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        let length = fs::metadata(&binary)?.len();
        if length == 0 || length > 64 * 1024 * 1024 {
            return Err("Standalone release CLI has an invalid size".into());
        }
        header.set_size(length);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "vmux", fs::File::open(&binary)?)?;
        tar.into_inner()?.finish()?.sync_all()?;
        let mut sign = self.command(
            self.environment
                .get("CARGO_BIN")
                .unwrap_or(OsStr::new("cargo")),
        );
        sign.args(["packager", "signer", "sign"])
            .arg(&archive)
            .arg("--private-key")
            .arg(self.environment.required("VMUX_UPDATE_PRIVATE_KEY")?)
            .arg("--password")
            .arg(
                self.environment
                    .get("VMUX_UPDATE_PRIVATE_KEY_PASSWORD")
                    .unwrap_or(OsStr::new("")),
            );
        Execution::new(sign).run()?;
        let signature = archive.with_file_name(format!("{name}.sig"));
        let key = String::from_utf8(
            STANDARD.decode(
                self.environment
                    .required("VMUX_UPDATE_PUBLIC_KEY")?
                    .as_encoded_bytes(),
            )?,
        )?;
        let signature_text =
            String::from_utf8(STANDARD.decode(fs::read_to_string(&signature)?.trim())?)?;
        minisign_verify::PublicKey::decode(&key)?.verify(
            &fs::read(&archive)?,
            &minisign_verify::Signature::decode(&signature_text)?,
            false,
        )?;
        let published = directory.join(&name);
        let published_signature = directory.join(format!("{name}.sig"));
        let mut publication = Publication::new(&temporary);
        publication.publish(&archive, &published)?;
        publication.publish(&signature, &published_signature)?;
        publication.commit();
        Ok(vec![published, published_signature])
    }

    pub(super) fn workspace_version(&self) -> Result<String> {
        let manifest: toml::Table =
            fs::read_to_string(self.checkout.root.join("Cargo.toml"))?.parse()?;
        Ok(manifest
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or("Missing workspace version")?
            .into())
    }

    pub fn nightly(&self, args: &[OsString]) -> Result<()> {
        if args.len() != 4 {
            return Err("nightly-version requires base, date, run number, and attempt".into());
        }
        let base = args[0].to_str().ok_or("Invalid base version")?;
        let parts = base.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err("Base version must be major.minor.patch".into());
        }
        let mut values = Vec::new();
        for part in parts {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Invalid base version".into());
            }
            values.push(part.parse::<u64>()?);
        }
        let date = args[1].to_str().ok_or("Invalid nightly date")?;
        if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Nightly date must be YYYYMMDD".into());
        }
        let mut counters = Vec::new();
        for arg in &args[2..] {
            let value = arg.to_str().ok_or("Invalid nightly counter")?;
            if value.is_empty()
                || !value.bytes().all(|byte| byte.is_ascii_digit())
                || value.len() > 1 && value.starts_with('0')
            {
                return Err("Nightly counters must be numeric without leading zeroes".into());
            }
            counters.push(value);
        }
        println!(
            "{}.{}.{}-nightly.{}.{}.{}",
            values[0],
            values[1],
            values[2].checked_add(1).ok_or("Version overflow")?,
            date,
            counters[0],
            counters[1]
        );
        Ok(())
    }

    pub fn release_artifacts(&self, version: &str) -> Result<()> {
        let values = self.artifact_values(version)?;
        let mut output = String::new();
        for (key, value) in values {
            output.push_str(&format!("{key}={value}\n"));
        }
        print!("{output}");
        if let Some(path) = self.environment.get("GITHUB_OUTPUT") {
            fs::OpenOptions::new()
                .append(true)
                .open(path)?
                .write_all(output.as_bytes())?;
        }
        Ok(())
    }

    fn artifact_values(&self, version: &str) -> Result<std::collections::BTreeMap<String, String>> {
        if version.is_empty()
            || !version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err("Invalid release version".into());
        }
        let repository = self
            .get("VMUX_RELEASE_REPOSITORY", "vmux-ai/vmux")
            .to_string_lossy()
            .into_owned();
        let tag = format!("v{version}");
        let base = format!("https://github.com/{repository}/releases/download/{tag}");
        let dmg = format!("Vmux_{version}_aarch64.dmg");
        let binaries = format!("vmux-v{version}-aarch64-apple-darwin.tar.gz");
        let app = format!("Vmux-v{version}-aarch64-apple-darwin.app.tar.gz");
        let cli = format!("vmux-cli-v{version}-aarch64-apple-darwin.tar.gz");
        let mut values = std::collections::BTreeMap::new();
        for (key, value) in [
            ("VMUX_RELEASE_VERSION", version.into()),
            ("VMUX_RELEASE_REPOSITORY", repository),
            ("VMUX_RELEASE_TAG", tag),
            ("VMUX_DMG_NAME", dmg.clone()),
            ("VMUX_BINARY_ARCHIVE_NAME", binaries),
            ("VMUX_APP_ARCHIVE_NAME", app.clone()),
            ("VMUX_APP_SIGNATURE_NAME", format!("{app}.sig")),
            ("VMUX_CLI_ARCHIVE_NAME", cli.clone()),
            ("VMUX_CLI_SIGNATURE_NAME", format!("{cli}.sig")),
            ("VMUX_CLI_URL", format!("{base}/{cli}")),
            ("VMUX_RELEASE_BASE_URL", base.clone()),
            ("VMUX_DMG_URL", format!("{base}/{dmg}")),
            ("VMUX_APP_URL", format!("{base}/{app}")),
        ] {
            values.insert(key.into(), value);
        }
        Ok(values)
    }

    pub fn release_metadata(&self, check: bool) -> Result<()> {
        let version = self.workspace_version()?;
        let values = self.artifact_values(&version)?;
        let tag = &values["VMUX_RELEASE_TAG"];
        let release: serde_json::Value = serde_json::from_slice(&self.output(
            "gh",
            &[
                OsStr::new("release"),
                OsStr::new("view"),
                OsStr::new(tag),
                OsStr::new("--json"),
                OsStr::new("isDraft,assets"),
            ],
        )?)?;
        let assets = release
            .get("assets")
            .and_then(serde_json::Value::as_array)
            .ok_or("Missing release assets")?;
        let mut found = std::collections::BTreeMap::new();
        for asset in assets {
            let name = asset
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or("Missing release asset name")?;
            if found.insert(name, asset).is_some() {
                return Err("Duplicate release asset".into());
            }
        }
        for key in [
            "VMUX_DMG_NAME",
            "VMUX_BINARY_ARCHIVE_NAME",
            "VMUX_APP_ARCHIVE_NAME",
            "VMUX_APP_SIGNATURE_NAME",
            "VMUX_CLI_ARCHIVE_NAME",
            "VMUX_CLI_SIGNATURE_NAME",
        ] {
            if !found.contains_key(values[key].as_str()) {
                return Err(format!("Release is missing {}", values[key]).into());
            }
        }
        let digest = found[values["VMUX_DMG_NAME"].as_str()]
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or("Missing DMG digest")?;
        let checksum = digest.strip_prefix("sha256:").ok_or("Invalid DMG digest")?;
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Invalid DMG checksum".into());
        }
        let temporary = Temporary::create(&std::env::temp_dir(), "vmux-release-metadata")?;
        self.run_command(
            "gh",
            &[
                OsStr::new("release"),
                OsStr::new("download"),
                OsStr::new(tag),
                OsStr::new("--pattern"),
                OsStr::new(&values["VMUX_APP_SIGNATURE_NAME"]),
                OsStr::new("--dir"),
                temporary.path().as_os_str(),
            ],
        )?;
        let signature =
            fs::read_to_string(temporary.path().join(&values["VMUX_APP_SIGNATURE_NAME"]))?;
        if signature.trim().is_empty() {
            return Err("Empty update signature".into());
        }
        self.run_command(
            "gh",
            &[
                OsStr::new("release"),
                OsStr::new("download"),
                OsStr::new(tag),
                OsStr::new("--pattern"),
                OsStr::new(&values["VMUX_CLI_SIGNATURE_NAME"]),
                OsStr::new("--dir"),
                temporary.path().as_os_str(),
            ],
        )?;
        let cli_signature =
            fs::read_to_string(temporary.path().join(&values["VMUX_CLI_SIGNATURE_NAME"]))?;
        if cli_signature.trim().is_empty() {
            return Err("Empty CLI update signature".into());
        }
        let manifest_path = self.checkout.root.join("website/public/updates.json");
        let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        let original_manifest = manifest.clone();
        let date = if manifest.get("version").and_then(serde_json::Value::as_str) == Some(tag) {
            manifest
                .get("pub_date")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(String::from)
        } else {
            None
        };
        let date = match date {
            Some(date) => date,
            None => found[values["VMUX_APP_ARCHIVE_NAME"].as_str()]
                .get("createdAt")
                .and_then(serde_json::Value::as_str)
                .ok_or("Missing app asset date")?
                .into(),
        };
        manifest["version"] = tag.clone().into();
        manifest["pub_date"] = date.into();
        manifest["platforms"]["macos-aarch64"]["url"] = values["VMUX_APP_URL"].clone().into();
        manifest["platforms"]["macos-aarch64"]["signature"] = signature.into();
        manifest["platforms"]["macos-aarch64"]["format"] = "app".into();
        manifest["cli"]["macos-aarch64"]["url"] = values["VMUX_CLI_URL"].clone().into();
        manifest["cli"]["macos-aarch64"]["signature"] = cli_signature.into();
        manifest["downloads"]["macos-aarch64"]["url"] = values["VMUX_DMG_URL"].clone().into();
        manifest["downloads"]["macos-aarch64"]["filename"] = values["VMUX_DMG_NAME"].clone().into();
        let cask_path = self.checkout.root.join("Casks/vmux.rb");
        let cask = fs::read_to_string(&cask_path)?;
        let mut updated = String::new();
        for line in cask.lines() {
            if line.starts_with("  version ") {
                updated.push_str(&format!("  version \"{version}\"\n"));
            } else if line.starts_with("  sha256 ") {
                updated.push_str(&format!("  sha256 \"{checksum}\"\n"));
            } else if line.starts_with("  url ") {
                updated.push_str(&format!("  url \"{}\"\n", values["VMUX_DMG_URL"]));
            } else {
                updated.push_str(line);
                updated.push('\n');
            }
        }
        if check {
            if manifest != original_manifest || updated != cask {
                return Err(
                    "Release metadata differs; run vmux internal update-release-metadata".into(),
                );
            }
            return Ok(());
        }
        fs::write(cask_path, updated)?;
        fs::write(
            manifest_path,
            format!("{}\n", serde_json::to_string_pretty(&manifest)?),
        )?;
        Ok(())
    }
}
