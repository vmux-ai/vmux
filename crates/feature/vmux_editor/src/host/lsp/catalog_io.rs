use std::fmt::Write;
use std::io::{Cursor, Read};

use bevy::prelude::Component;
use sha2::{Digest, Sha256};
use vmux_job::{BlockingCancellation, Job};

use super::package_path::Sha256Digest;

#[derive(Clone)]
pub(super) struct DecodeArchive {
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: Sha256Digest,
}

impl Job for DecodeArchive {
    type Output = Result<RegistryBytes, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("registry archive decode canceled".into());
        }
        if self.bytes.len() > 32 * 1024 * 1024 {
            return Err("registry archive exceeds 33554432 bytes".into());
        }
        let digest = Sha256::digest(&self.bytes);
        let mut hex = String::with_capacity(64);
        for byte in digest {
            write!(&mut hex, "{byte:02x}").unwrap();
        }
        if hex != self.sha256.as_str() {
            return Err("registry archive checksum mismatch".into());
        }
        let mut archive =
            zip::ZipArchive::new(Cursor::new(self.bytes)).map_err(|error| error.to_string())?;
        let mut entry = archive
            .by_name("registry.json")
            .map_err(|error| error.to_string())?;
        if entry.size() > 32 * 1024 * 1024 {
            return Err("registry document exceeds 33554432 bytes".into());
        }
        let mut bytes = Vec::new();
        let mut buffer = [0; 65536];
        loop {
            if BlockingCancellation::canceled() {
                return Err("registry archive decode canceled".into());
            }
            let read = entry.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) > 32 * 1024 * 1024 {
                return Err("registry document exceeds 33554432 bytes".into());
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        Ok(RegistryBytes(bytes))
    }
}

#[derive(Component)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(super) struct RegistryBytes(pub(super) Vec<u8>);
