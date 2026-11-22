use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vmux_path::Executable;

use crate::lsp::package_path::{PackageName, PackagePath};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageStore {
    root: PathBuf,
}

#[derive(bevy::prelude::Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub name: PackageName,
    pub version: Option<String>,
    pub source_id: String,
    pub bin: BTreeMap<PackageName, PackagePath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Managed(PathBuf),
    OnPath,
    Missing,
}

impl PackageStore {
    const RECEIPT_MAX_BYTES: u64 = 1024 * 1024;

    pub fn lsp() -> Self {
        Self::at(vmux_ecs::profile::ProfilePaths::current().lsp())
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn registries_dir(&self) -> PathBuf {
        self.root.join("registries")
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.registries_dir().join("registry.json")
    }

    pub fn package_dir(&self, name: &PackageName) -> PathBuf {
        self.packages_dir().join(name.as_str())
    }

    pub fn write_receipt(&self, name: &PackageName, receipt: &Receipt) -> io::Result<()> {
        let directory = self.package_dir(name);
        std::fs::create_dir_all(&directory)?;
        receipt.write_to(&directory)
    }

    pub fn read_receipt(&self, name: &PackageName) -> Option<Receipt> {
        let file = std::fs::File::open(self.receipt_path(name)).ok()?;
        let mut bytes = Vec::new();
        file.take(Self::RECEIPT_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > Self::RECEIPT_MAX_BYTES {
            return None;
        }
        let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
        (receipt.name == *name).then_some(receipt)
    }

    pub fn is_installed(&self, name: &PackageName) -> bool {
        self.receipt_path(name).is_file()
    }

    pub fn link_bin(
        &self,
        name: &PackageName,
        file: &PackagePath,
        link_name: &PackageName,
    ) -> io::Result<()> {
        let bin = self.bin_dir();
        std::fs::create_dir_all(&bin)?;
        let link = bin.join(link_name.as_str());
        let target = self.package_dir(name).join(file.as_path());
        if !target.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("package binary does not exist: {}", target.display()),
            ));
        }
        let temporary = bin.join(format!(
            ".{}.{}.tmp",
            link_name.as_str(),
            std::process::id()
        ));
        let _ = std::fs::remove_file(&temporary);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &temporary)?;
        }
        #[cfg(not(unix))]
        {
            std::fs::copy(&target, &temporary)?;
        }
        std::fs::rename(temporary, link)?;
        Ok(())
    }

    pub fn bin_path(&self, name: &PackageName) -> Option<PathBuf> {
        let receipt = self.read_receipt(name)?;
        let link_name = receipt.bin.keys().next()?;
        let path = self.bin_dir().join(link_name.as_str());
        path.exists().then_some(path)
    }

    pub fn remove_package_dir(&self, name: &PackageName) -> io::Result<()> {
        let directory = self.package_dir(name);
        if directory.exists() {
            std::fs::remove_dir_all(directory)?;
        }
        Ok(())
    }

    pub fn server_path_env(&self) -> std::ffi::OsString {
        let mut parts = vec![self.bin_dir()];
        if let Some(path) = std::env::var_os("PATH") {
            parts.extend(std::env::split_paths(&path));
        }
        std::env::join_paths(parts).unwrap_or_default()
    }

    pub fn resolve_command(&self, command: &str) -> Resolution {
        let managed = self.bin_dir().join(command);
        if managed.is_file() || managed.is_symlink() {
            return Resolution::Managed(managed);
        }
        if Executable::find(command).is_some() {
            return Resolution::OnPath;
        }
        Resolution::Missing
    }

    fn receipt_path(&self, name: &PackageName) -> PathBuf {
        self.package_dir(name).join("vmux-receipt.json")
    }
}

impl Receipt {
    pub fn write_to(&self, package_dir: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        vmux_path::AtomicFile::write(package_dir.join("vmux-receipt.json"), &json)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    impl Receipt {
        fn sample(name: &str) -> Self {
            let mut bin = BTreeMap::new();
            bin.insert(
                PackageName::parse(name).unwrap(),
                PackagePath::parse(&format!("{name}-bin")).unwrap(),
            );
            Self {
                name: PackageName::parse(name).unwrap(),
                version: Some("1.0".into()),
                source_id: "pkg:github/x/y@1.0".into(),
                bin,
            }
        }
    }

    #[test]
    fn write_read_installed_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let pkgdir = store.package_dir(&name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"#!/bin/sh\n").unwrap();
        store.write_receipt(&name, &Receipt::sample("foo")).unwrap();

        assert!(store.is_installed(&name));
        assert_eq!(
            store.read_receipt(&name).unwrap().version.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn removing_a_package_directory_preserves_the_bin_namespace() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let bin_name = PackageName::parse("foo").unwrap();
        let package_binary = PackagePath::parse("foo-bin").unwrap();
        let pkgdir = store.package_dir(&name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        store.write_receipt(&name, &Receipt::sample("foo")).unwrap();
        store.link_bin(&name, &package_binary, &bin_name).unwrap();

        assert!(store.bin_path(&name).is_some());
        store.remove_package_dir(&name).unwrap();
        assert!(!store.is_installed(&name));
        assert!(store.bin_dir().join("foo").is_symlink());
    }

    #[test]
    fn resolution_prefers_managed_then_path_then_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let bin_name = PackageName::parse("foo").unwrap();
        let package_binary = PackagePath::parse("foo-bin").unwrap();
        let pkgdir = store.package_dir(&name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        store.write_receipt(&name, &Receipt::sample("foo")).unwrap();
        store.link_bin(&name, &package_binary, &bin_name).unwrap();
        assert!(matches!(
            store.resolve_command("foo"),
            Resolution::Managed(_)
        ));
        assert_eq!(store.resolve_command("cargo"), Resolution::OnPath);
        assert_eq!(
            store.resolve_command("definitely-not-real-zzz"),
            Resolution::Missing
        );
    }

    #[test]
    fn invalid_or_mismatched_receipts_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let dir = store.package_dir(&name);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("vmux-receipt.json"),
            r#"{"name":"foo","version":null,"source_id":"x","bin":{"../../escape":"bin"}}"#,
        )
        .unwrap();
        assert!(store.read_receipt(&name).is_none());

        std::fs::write(
            dir.join("vmux-receipt.json"),
            r#"{"name":"bar","version":null,"source_id":"x","bin":{}}"#,
        )
        .unwrap();
        assert!(store.read_receipt(&name).is_none());
    }
}
