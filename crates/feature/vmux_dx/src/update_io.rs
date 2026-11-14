use std::fs::{self, File, OpenOptions, Permissions, TryLockError};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::Ordering};
use std::thread;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use bevy_ecs::prelude::Component;
use flate2::read::GzDecoder;

use crate::Result;
use crate::environment::Environment;
use crate::execution::{Execution, Failure};
use crate::signals::Signals;
use crate::temporary::Temporary;
use crate::updates::{Channel, Download, Manifest, Preferences};

#[derive(Component, Clone)]
pub(crate) struct Installation {
    pub(crate) executable: PathBuf,
    pub(crate) root: PathBuf,
    pub(crate) version: String,
    pub(crate) platform: String,
    pub(crate) key: String,
    pub(crate) release_compatible: bool,
    pub(crate) environment: Environment,
}

impl Installation {
    pub(crate) fn current() -> Result<Self> {
        let environment = Environment::current();
        let root = match environment.get("VMUX_CLI_UPDATE_ROOT") {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(environment.required("HOME")?).join(".vmux/cli"),
        };
        let key = match environment.get("VMUX_UPDATE_PUBLIC_KEY") {
            Some(key) => key.to_string_lossy().into_owned(),
            None => option_env!("VMUX_UPDATE_PUBLIC_KEY")
                .unwrap_or_default()
                .to_string(),
        };
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-aarch64",
            ("macos", "x86_64") => "macos-x86_64",
            ("linux", "x86_64") => "linux-x86_64",
            ("linux", "aarch64") => "linux-aarch64",
            _ => "unsupported",
        };
        Ok(Self {
            executable: std::env::current_exe()?.canonicalize()?,
            root,
            version: env!("CARGO_PKG_VERSION").into(),
            platform: platform.into(),
            key,
            release_compatible: !cfg!(feature = "runtime-cli"),
            environment,
        })
    }

    pub(crate) fn standalone(&self) -> bool {
        if !self.release_compatible {
            return false;
        }
        if self.executable.parent().is_some_and(|parent| {
            parent.join(".cargo-lock").exists() || parent.join(".fingerprint").is_dir()
        }) {
            return false;
        }
        for component in self.executable.components() {
            let name = component.as_os_str();
            if name == "target"
                || name == ".build-tools"
                || Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension == "app")
            {
                return false;
            }
        }
        self.executable
            .file_name()
            .is_some_and(|name| name == "vmux")
    }

    pub(crate) fn trusted(&self) -> bool {
        !self.key.trim().is_empty()
    }

    fn directory(&self) -> Result<()> {
        if !self.root.is_absolute() {
            return Err("CLI update storage must be an absolute path".into());
        }
        if fs::symlink_metadata(&self.root).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("CLI update storage must not be a symlink".into());
        }
        fs::create_dir_all(&self.root)?;
        fs::set_permissions(&self.root, Permissions::from_mode(0o700))?;
        Ok(())
    }

    pub(crate) fn lock(&self, nonblocking: bool) -> Result<Option<Guard>> {
        self.directory()?;
        let path = self.root.join("update.lock");
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("CLI update lock must not be a symlink".into());
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        loop {
            Signals::check()?;
            match file.try_lock() {
                Ok(()) => return Ok(Some(Guard(file))),
                Err(TryLockError::WouldBlock) if nonblocking => return Ok(None),
                Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(20)),
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    }

    pub(crate) fn load(&self) -> Result<Preferences> {
        let path = self.root.join("update.json");
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("CLI update preferences must not be a symlink".into());
        }
        match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(16385).read_to_end(&mut bytes)?;
                if bytes.len() > 16384 {
                    return Err("CLI update preferences exceed 16 KiB".into());
                }
                Ok(serde_json::from_slice(&bytes)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Preferences::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn save(&self, preferences: &Preferences) -> Result<()> {
        self.directory()?;
        let temporary = Temporary::create(&self.root, ".preferences")?;
        let source = temporary.path().join("update.json");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&source)?;
        file.write_all(&serde_json::to_vec(preferences)?)?;
        file.sync_all()?;
        fs::rename(source, self.root.join("update.json"))?;
        Ok(())
    }

    pub(crate) fn check(&self, channel: Channel) -> Result<Manifest> {
        let mut command = self.environment.command(
            self.environment
                .get("CURL_BIN")
                .unwrap_or(std::ffi::OsStr::new("curl")),
            &self.root,
        );
        command.args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "5",
            "--max-time",
            "10",
            "--max-filesize",
            "1048576",
            "--",
            channel.endpoint(),
        ]);
        let output = Execution::new(command).output(None)?;
        if output.stdout.len() > 1024 * 1024 {
            return Err("CLI release manifest exceeds 1 MiB".into());
        }
        let manifest: Manifest = serde_json::from_slice(&output.stdout)?;
        if manifest.cli.len() > 16 {
            return Err("CLI release manifest has too many platform entries".into());
        }
        Ok(manifest)
    }

    pub(crate) fn download(&self, version: &str, download: &Download) -> Result<Verified> {
        if !self.trusted() {
            return Err("This source-built CLI has no release verification key; use a signed standalone CLI release or rebuild with VMUX_UPDATE_PUBLIC_KEY".into());
        }
        let parent = self
            .executable
            .parent()
            .ok_or("Missing CLI installation directory")?;
        let temporary = Arc::new(Temporary::create(parent, ".vmux-cli-update")?);
        let archive = temporary.path().join("cli.tar.gz");
        let mut command = self.environment.command(
            self.environment
                .get("CURL_BIN")
                .unwrap_or(std::ffi::OsStr::new("curl")),
            &self.root,
        );
        command
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "5",
                "--max-time",
                "300",
                "--max-filesize",
                "67108864",
                "--output",
            ])
            .arg(&archive)
            .args(["--", &download.url]);
        unsafe {
            command.pre_exec(|| {
                let size = libc::rlimit {
                    rlim_cur: 64 * 1024 * 1024,
                    rlim_max: 64 * 1024 * 1024,
                };
                let core = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &size) != 0
                    || libc::setrlimit(libc::RLIMIT_CORE, &core) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Execution::new(command).run()?;
        let mut bytes = Vec::new();
        File::open(&archive)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("CLI release archive exceeds 64 MiB".into());
        }
        let signature = String::from_utf8(STANDARD.decode(download.signature.trim())?)?;
        let signature = minisign_verify::Signature::decode(&signature)?;
        let key_bytes = STANDARD.decode(self.key.trim())?;
        let key_text = String::from_utf8(key_bytes)?;
        let key = minisign_verify::PublicKey::decode(&key_text)?;
        key.verify(&bytes, &signature, false)?;
        let path = temporary.path().join("vmux");
        Self::extract(&bytes, &path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o755))?;
        let mut probe = self.environment.command(&path, temporary.path());
        probe.arg("--version").env("VMUX_CLI_UPDATE_HANDOFF", "1");
        let output = Execution::new(probe).output(None)?;
        let expected = format!("vmux {}", version.trim_start_matches('v'));
        if std::str::from_utf8(&output.stdout)?.trim() != expected {
            return Err("CLI archive executable does not match its release version".into());
        }
        Ok(Verified {
            path,
            temporary,
            version: version.into(),
        })
    }

    fn extract(bytes: &[u8], destination: &Path) -> Result<()> {
        let reader = GzDecoder::new(bytes).take(65 * 1024 * 1024);
        let mut archive = tar::Archive::new(reader);
        let mut found = false;
        for entry in archive.entries()? {
            let mut entry = entry?;
            if found
                || entry.path()?.as_ref() != Path::new("vmux")
                || !entry.header().entry_type().is_file()
            {
                return Err("CLI archive must contain exactly one regular file named vmux".into());
            }
            if entry.size() == 0 || entry.size() > 64 * 1024 * 1024 {
                return Err("CLI archive executable has an invalid size".into());
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(destination)?;
            std::io::copy(&mut entry, &mut file)?;
            file.sync_all()?;
            found = true;
        }
        if !found {
            return Err("CLI release archive contains no executable".into());
        }
        Ok(())
    }

    pub(crate) fn publish(&self, verified: &Verified) -> Result<Published> {
        Signals::check()?;
        if fs::symlink_metadata(&self.executable)?.is_symlink() {
            return Err("CLI installation changed to a symlink during update".into());
        }
        let backup = verified.temporary.path().join("previous");
        fs::copy(&self.executable, &backup)?;
        File::open(&backup)?.sync_all()?;
        fs::rename(&verified.path, &self.executable)?;
        Ok(Published {
            executable: self.executable.clone(),
            backup,
            temporary: verified.temporary.clone(),
            version: verified.version.clone(),
        })
    }

    pub(crate) fn handoff(&self, published: &Published) -> Result<u8> {
        let mut command = self
            .environment
            .command(&self.executable, &std::env::current_dir()?);
        command
            .args(std::env::args_os().skip(1))
            .env("VMUX_CLI_UPDATE_HANDOFF", "1");
        let result = Execution::new(command).run();
        let code = match result {
            Ok(()) => 0,
            Err(error) => match error.downcast::<Failure>() {
                Ok(error) => error.code() as u8,
                Err(error) => {
                    published.restore()?;
                    return Err(error);
                }
            },
        };
        if let Err(error) = published.complete() {
            eprintln!("CLI update backup retained: {error}");
        }
        Ok(code)
    }
}

pub(crate) struct Guard(File);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Clone)]
pub(crate) struct Verified {
    path: PathBuf,
    temporary: Arc<Temporary>,
    version: String,
}

#[derive(Clone)]
pub(crate) struct Published {
    executable: PathBuf,
    backup: PathBuf,
    temporary: Arc<Temporary>,
    version: String,
}

impl Published {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn complete(&self) -> Result<()> {
        if let Err(error) = fs::remove_dir_all(self.temporary.path()) {
            self.temporary.retention().store(true, Ordering::Release);
            return Err(format!("{}: {error}", self.temporary.path().display()).into());
        }
        Ok(())
    }

    fn restore(&self) -> Result<()> {
        if let Err(error) = fs::rename(&self.backup, &self.executable) {
            self.temporary.retention().store(true, Ordering::Release);
            return Err(format!(
                "CLI rollback failed; backup retained at {}: {error}",
                self.backup.display()
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_archives_refuse_links_duplicate_members_and_excessive_declared_sizes() {
        let directory = tempfile::tempdir().unwrap();
        for (kind, duplicate, length) in [
            (tar::EntryType::Symlink, false, 0),
            (tar::EntryType::Regular, true, 1),
            (tar::EntryType::Regular, false, 64 * 1024 * 1024 + 1),
        ] {
            let mut bytes = Vec::new();
            let mut header = tar::Header::new_gnu();
            header.set_path("vmux").unwrap();
            header.set_entry_type(kind);
            header.set_size(length);
            header.set_mode(0o755);
            header.set_cksum();
            bytes.extend_from_slice(header.as_bytes());
            if length == 1 {
                bytes.extend_from_slice(&[42; 512]);
            }
            if duplicate {
                bytes.extend_from_slice(header.as_bytes());
                bytes.extend_from_slice(&[42; 512]);
            }
            bytes.extend_from_slice(&[0; 1024]);
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&bytes).unwrap();
            let archive = encoder.finish().unwrap();
            let destination = directory.path().join(format!("vmux-{length}-{duplicate}"));
            assert!(Installation::extract(&archive, &destination).is_err());
        }
    }

    #[test]
    fn published_cli_keeps_a_recoverable_backup_until_handoff_and_retains_failed_rollback() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("vmux");
        fs::write(&executable, b"original CLI").unwrap();
        let installation = Installation {
            executable: executable.clone(),
            root: directory.path().join("updates"),
            version: "1.0.0".into(),
            platform: "macos-aarch64".into(),
            release_compatible: true,
            key: String::new(),
            environment: Environment::current(),
        };
        let temporary = Arc::new(Temporary::create(directory.path(), ".verified").unwrap());
        let source = temporary.path().join("vmux");
        fs::write(&source, b"verified new CLI").unwrap();
        let verified = Verified {
            path: source,
            temporary: temporary.clone(),
            version: "2.0.0".into(),
        };
        let published = installation.publish(&verified).unwrap();
        assert_eq!(fs::read(&executable).unwrap(), b"verified new CLI");
        assert_eq!(fs::read(&published.backup).unwrap(), b"original CLI");
        published.restore().unwrap();
        assert_eq!(fs::read(&executable).unwrap(), b"original CLI");

        let source = temporary.path().join("next");
        fs::write(&source, b"verified next CLI").unwrap();
        let verified = Verified {
            path: source,
            temporary: temporary.clone(),
            version: "3.0.0".into(),
        };
        let published = installation.publish(&verified).unwrap();
        fs::remove_file(&executable).unwrap();
        fs::create_dir(&executable).unwrap();
        let backup = published.backup.clone();
        assert!(published.restore().is_err());
        drop(published);
        drop(verified);
        drop(temporary);
        assert_eq!(fs::read(backup).unwrap(), b"original CLI");
    }
}
