use std::io;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::Settings;
use crate::http::UnixHttp;

pub(crate) struct Ssh;

impl Ssh {
    pub(crate) fn prepare(settings: &Settings) -> io::Result<()> {
        let key = settings.directory.join("ssh-key");
        if !key.exists() {
            let status = Command::new("ssh-keygen")
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
                .arg(&key)
                .status()?;
            if !status.success() {
                return Err(io::Error::other(format!("create engine SSH key: {status}")));
            }
        }
        let authorized = settings.rootfs().join("root/.ssh");
        std::fs::create_dir_all(&authorized)?;
        std::fs::copy(
            key.with_file_name("ssh-key.pub"),
            authorized.join("authorized_keys"),
        )?;
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
