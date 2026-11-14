use std::io;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::Settings;
use crate::client::Preparation;
use crate::http::UnixHttp;

pub(crate) struct Ssh;

impl Ssh {
    pub(crate) fn prepare(settings: &Settings, preparation: &Preparation<'_>) -> io::Result<()> {
        preparation.check()?;
        let key = settings.directory.join("ssh-key");
        let public = settings.directory.join("ssh-key.pub");
        if !key.exists() || !public.exists() {
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
                    .arg(&staged_key),
            )?;
            if !status.success() {
                return Err(io::Error::other(format!("create engine SSH key: {status}")));
            }
            preparation.check()?;
            std::fs::rename(staging.path().join("ssh-key.pub"), &public)?;
            std::fs::rename(staged_key, &key)?;
        }
        let authorized = settings.rootfs().join("root/.ssh");
        preparation.check()?;
        std::fs::create_dir_all(&authorized)?;
        std::fs::copy(public, authorized.join("authorized_keys"))?;
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600))?;
        std::fs::set_permissions(&authorized, std::fs::Permissions::from_mode(0o700))?;
        std::fs::set_permissions(
            authorized.join("authorized_keys"),
            std::fs::Permissions::from_mode(0o600),
        )?;
        std::fs::create_dir_all(settings.rootfs().join("etc/ssh"))?;
        std::fs::write(settings.rootfs().join("etc/ssh/sshd_config"), b"Port 22\nListenAddress 192.168.127.2\nPermitRootLogin prohibit-password\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nAuthorizedKeysFile /run/vmux-authorized-keys\nAllowUsers root\n")?;
        Ok(())
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
    fn preparation_recovers_from_an_incomplete_key_pair() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
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
