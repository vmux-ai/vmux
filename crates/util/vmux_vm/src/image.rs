use std::fmt::Write;
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::process::Command;

use flate2::read::GzDecoder;
use ring::digest::{Context, SHA256};

use crate::Settings;
use crate::client::Preparation;
use crate::root::Root;

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
        preparation.check()?;
        let root = Root::open(settings)?;
        root.write(
            "sbin/vmux-init",
            include_str!("guest-init.sh").as_bytes(),
            0o755,
        )?;
        root.write(
            "sbin/vmux-control",
            include_str!("guest-control.sh").as_bytes(),
            0o755,
        )?;
        Self::prepare_disk(settings, preparation)?;
        Ok(())
    }

    fn prepare_disk(settings: &Settings, preparation: &Preparation<'_>) -> io::Result<()> {
        preparation.check()?;
        let marker = settings.directory.join("data.new");
        match std::fs::metadata(settings.disk()) {
            Ok(metadata) if metadata.len() > 0 => {
                if metadata.blocks() == 0 && !marker.exists() {
                    let mut pending = tempfile::NamedTempFile::new_in(&settings.directory)?;
                    io::Write::write_all(&mut pending, b"new")?;
                    pending.as_file().sync_all()?;
                    pending.persist(marker).map_err(io::Error::other)?;
                }
                return Ok(());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let disk = tempfile::NamedTempFile::new_in(&settings.directory)?;
        disk.as_file()
            .set_len(settings.disk_gib * 1024 * 1024 * 1024)?;
        disk.as_file().sync_all()?;
        let mut pending = tempfile::NamedTempFile::new_in(&settings.directory)?;
        io::Write::write_all(&mut pending, b"new")?;
        pending.as_file().sync_all()?;
        preparation.check()?;
        pending.persist(marker).map_err(io::Error::other)?;
        preparation.check()?;
        disk.persist(settings.disk()).map_err(io::Error::other)?;
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
    fn disk_initialization_failure_publishes_nothing_and_retry_recovers() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.disk_gib = 1;
        settings.validate().unwrap();
        let marker = settings.directory.join("data.new");
        std::fs::create_dir(&marker).unwrap();
        assert!(GuestImage::prepare_disk(&settings, &Preparation::new(None)).is_err());
        assert!(!settings.disk().exists());
        std::fs::remove_dir(&marker).unwrap();
        GuestImage::prepare_disk(&settings, &Preparation::new(None)).unwrap();
        assert_eq!(
            std::fs::metadata(settings.disk()).unwrap().len(),
            1024 * 1024 * 1024
        );
        assert_eq!(std::fs::read(marker).unwrap(), b"new");
    }

    #[test]
    fn disk_preparation_repairs_empty_legacy_disks_and_preserves_existing_data() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(directory.path().join("engine"), directory.path());
        settings.disk_gib = 1;
        settings.validate().unwrap();
        std::fs::write(settings.disk(), b"").unwrap();
        GuestImage::prepare_disk(&settings, &Preparation::new(None)).unwrap();
        assert_eq!(
            std::fs::metadata(settings.disk()).unwrap().len(),
            1024 * 1024 * 1024
        );
        std::fs::write(settings.disk(), b"existing disk contents").unwrap();
        std::fs::remove_file(settings.directory.join("data.new")).unwrap();
        GuestImage::prepare_disk(&settings, &Preparation::new(None)).unwrap();
        assert_eq!(
            std::fs::read(settings.disk()).unwrap(),
            b"existing disk contents"
        );
        assert!(!settings.directory.join("data.new").exists());
    }

    #[test]
    fn corrupted_guest_image_is_refused_before_extraction() {
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        GuestImage::verify(&b"abc"[..], digest).unwrap();
        let error = GuestImage::verify(&b"abd"[..], digest).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
