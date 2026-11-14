use std::fmt::Write;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use flate2::read::GzDecoder;
use ring::digest::{Context, SHA256};

use crate::Settings;
use crate::client::Preparation;

pub(crate) struct GuestImage;

impl GuestImage {
    const URL: &str = "https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/alpine-minirootfs-3.24.4-aarch64.tar.gz";
    const SHA256: &str = "4b14c18f7e90a063b61462ea1462a5732c154bf6adad594974fe6487cd98a8de";

    pub(crate) fn prepare(settings: &Settings, preparation: &Preparation<'_>) -> io::Result<()> {
        preparation.check()?;
        if !settings.rootfs().join(".vmux-image").exists() {
            let archive = settings.directory.join("alpine.tar.gz");
            if !archive.exists() {
                let temporary = tempfile::NamedTempFile::new_in(&settings.directory)?;
                let status = preparation.run(
                    Command::new("curl")
                        .args([
                            "--fail",
                            "--location",
                            "--proto",
                            "=https",
                            "--max-time",
                            "180",
                            "--output",
                        ])
                        .arg(temporary.path())
                        .arg(Self::URL),
                )?;
                if !status.success() {
                    return Err(io::Error::other(format!("download guest image: {status}")));
                }
                Self::verify(File::open(temporary.path())?, Self::SHA256)?;
                temporary.persist(&archive).map_err(io::Error::other)?;
            }
            Self::verify(File::open(&archive)?, Self::SHA256)?;
            preparation.check()?;
            let staging = tempfile::tempdir_in(&settings.directory)?;
            let mut tar = tar::Archive::new(GzDecoder::new(File::open(archive)?));
            tar.set_preserve_permissions(true);
            tar.unpack(staging.path())?;
            preparation.check()?;
            std::fs::write(staging.path().join(".vmux-image"), Self::SHA256)?;
            std::fs::rename(staging.path(), settings.rootfs())?;
        }
        let init = settings.rootfs().join("sbin/vmux-init");
        preparation.check()?;
        std::fs::write(&init, include_str!("guest-init.sh"))?;
        let control = settings.rootfs().join("sbin/vmux-control");
        std::fs::write(&control, include_str!("guest-control.sh"))?;
        #[cfg(unix)]
        {
            std::fs::set_permissions(init, std::fs::Permissions::from_mode(0o755))?;
            std::fs::set_permissions(control, std::fs::Permissions::from_mode(0o755))?;
        }
        if !settings.disk().exists() {
            let disk = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(settings.disk())?;
            disk.set_len(settings.disk_gib * 1024 * 1024 * 1024)?;
            disk.sync_all()?;
            std::fs::write(settings.directory.join("data.new"), b"new")?;
        }
        Ok(())
    }

    fn verify(mut source: impl Read, expected: &str) -> io::Result<()> {
        let mut digest = Context::new(&SHA256);
        let mut buffer = [0; 65536];
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        let mut actual = String::new();
        for byte in digest.finish().as_ref() {
            write!(&mut actual, "{byte:02x}").map_err(io::Error::other)?;
        }
        if actual != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "guest image checksum mismatch",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupted_guest_image_is_refused_before_extraction() {
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        GuestImage::verify(&b"abc"[..], digest).unwrap();
        let error = GuestImage::verify(&b"abd"[..], digest).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
