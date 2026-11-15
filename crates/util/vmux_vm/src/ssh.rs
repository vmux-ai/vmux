use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::Settings;
use crate::client::Preparation;
use crate::http::UnixHttp;
use crate::root::Root;

pub(crate) struct Ssh;

impl Ssh {
    pub(crate) fn prepare(settings: &Settings, preparation: &Preparation<'_>) -> io::Result<()> {
        preparation.check()?;
        let key = settings.directory.join("ssh-key");
        let public = settings.directory.join("ssh-key.pub");
        let derived = tempfile::NamedTempFile::new_in(&settings.directory)?;
        if key.exists() {
            let metadata = std::fs::symlink_metadata(&key)?;
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "engine SSH key is not a regular file",
                ));
            }
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))?;
        }
        if !Self::derive(&key, &derived, preparation)? {
            let staging = tempfile::tempdir_in(&settings.directory)?;
            let staged_key = staging.path().join("ssh-key");
            let status = preparation.run(
                Command::new("ssh-keygen")
                    .args([
                        "-q",
                        "-t",
                        "ed25519",
                        "-N",
                        "",
                        "-C",
                        "vmux-container",
                        "-f",
                    ])
                    .arg(&staged_key)
                    .stdin(Stdio::null()),
            )?;
            if !status.success() {
                return Err(io::Error::other(format!("create engine SSH key: {status}")));
            }
            preparation.check()?;
            std::fs::rename(staged_key, &key)?;
            derived.as_file().set_len(0)?;
            if !Self::derive(&key, &derived, preparation)? {
                return Err(io::Error::other("derive engine SSH public key"));
            }
        }
        preparation.check()?;
        derived.as_file().sync_all()?;
        derived.persist(&public).map_err(io::Error::other)?;
        preparation.check()?;
        let root = Root::open(settings)?;
        root.directory("root/.ssh", 0o700)?;
        root.write("root/.ssh/authorized_keys", &std::fs::read(public)?, 0o600)?;
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))?;
        root.write("etc/ssh/sshd_config", b"Port 22\nListenAddress 192.168.127.2\nPermitRootLogin prohibit-password\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nAuthorizedKeysFile /run/vmux-authorized-keys\nAllowUsers root\n", 0o600)?;
        Ok(())
    }

    fn derive(
        key: &Path,
        output: &tempfile::NamedTempFile,
        preparation: &Preparation<'_>,
    ) -> io::Result<bool> {
        if !key.exists() {
            return Ok(false);
        }
        let status = preparation.run(
            Command::new("ssh-keygen")
                .args(["-y", "-P", "", "-f"])
                .arg(key)
                .stdin(Stdio::null())
                .stdout(output.reopen()?)
                .stderr(Stdio::null()),
        )?;
        Ok(status.success())
    }

    pub(crate) fn forward(settings: &Settings) -> io::Result<()> {
        let mut remote = url::Url::parse("ssh-tunnel://root@192.168.127.2:22/run/docker.sock")
            .map_err(io::Error::other)?;
        remote
            .query_pairs_mut()
            .append_pair("key", &settings.directory.join("ssh-key").to_string_lossy());
        let body = serde_json::to_vec(&serde_json::json!({
            "protocol": "unix",
            "local": settings.socket(),
            "remote": remote.as_str(),
        }))
        .map_err(io::Error::other)?;
        UnixHttp::new(&settings.network_api())
            .post("/services/forwarder/expose", &body)?
            .require_success()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_public_keys_are_repaired_without_rotating_a_valid_private_key() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        Ssh::prepare(&settings, &Preparation::new(None)).unwrap();
        let private = std::fs::read(settings.directory.join("ssh-key")).unwrap();
        let public = std::fs::read(settings.directory.join("ssh-key.pub")).unwrap();
        std::fs::write(settings.directory.join("ssh-key.pub"), b"stale public key").unwrap();
        Ssh::prepare(&settings, &Preparation::new(None)).unwrap();
        assert_eq!(
            std::fs::read(settings.directory.join("ssh-key")).unwrap(),
            private
        );
        assert_eq!(
            std::fs::read(settings.directory.join("ssh-key.pub")).unwrap(),
            public
        );
        assert_eq!(
            std::fs::read(settings.rootfs().join("root/.ssh/authorized_keys")).unwrap(),
            public
        );
    }

    #[test]
    fn preparation_recovers_from_an_incomplete_key_pair() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        let key = settings.directory.join("ssh-key");
        std::fs::write(&key, "incomplete private key").unwrap();
        Ssh::prepare(&settings, &Preparation::new(None)).unwrap();
        let output = Command::new("ssh-keygen")
            .args(["-y", "-f"])
            .arg(&key)
            .output()
            .unwrap();
        assert!(output.status.success());
        let authorized =
            std::fs::read(settings.rootfs().join("root/.ssh/authorized_keys")).unwrap();
        let derived = String::from_utf8(output.stdout).unwrap();
        assert!(
            String::from_utf8(authorized)
                .unwrap()
                .starts_with(derived.trim())
        );
    }
}
