use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lsp::package_path::{PackageName, PackagePath};

const RECEIPT_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageStore {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn activate_package(&self, name: &PackageName, staged: &Path) -> io::Result<()> {
        let packages = self.packages_dir();
        std::fs::create_dir_all(&packages)?;
        let target = self.package_dir(name);
        let backup =
            self.staging_dir()
                .join(format!(".{}.{}.backup", name.as_str(), std::process::id()));
        let _ = std::fs::remove_dir_all(&backup);
        if target.exists() {
            std::fs::rename(&target, &backup)?;
        }
        if let Err(error) = std::fs::rename(staged, &target) {
            if backup.exists() {
                let _ = std::fs::rename(&backup, &target);
            }
            return Err(error);
        }
        if backup.exists() {
            std::fs::remove_dir_all(backup)?;
        }
        Ok(())
    }

    pub fn read_receipt(&self, name: &PackageName) -> Option<Receipt> {
        let file = std::fs::File::open(self.receipt_path(name)).ok()?;
        let mut bytes = Vec::new();
        file.take(RECEIPT_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > RECEIPT_MAX_BYTES {
            return None;
        }
        let receipt: Receipt = serde_json::from_slice(&bytes).ok()?;
        (receipt.name == *name).then_some(receipt)
    }

    pub fn installed(&self) -> BTreeMap<PackageName, Receipt> {
        let mut receipts = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(self.packages_dir()) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && let Ok(name) = PackageName::parse(name)
                    && let Some(receipt) = self.read_receipt(&name)
                {
                    receipts.insert(name, receipt);
                }
            }
        }
        receipts
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

    pub fn remove(&self, name: &PackageName) -> io::Result<()> {
        if let Some(receipt) = self.read_receipt(name) {
            for link_name in receipt.bin.keys() {
                let _ = std::fs::remove_file(self.bin_dir().join(link_name.as_str()));
            }
        }
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
        if crate::lsp::registry::executable_on_path(command) {
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

    fn receipt(name: &str) -> Receipt {
        let mut bin = BTreeMap::new();
        bin.insert(
            PackageName::parse(name).unwrap(),
            PackagePath::parse(&format!("{name}-bin")).unwrap(),
        );
        Receipt {
            name: PackageName::parse(name).unwrap(),
            version: Some("1.0".into()),
            source_id: "pkg:github/x/y@1.0".into(),
            bin,
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
        store.write_receipt(&name, &receipt("foo")).unwrap();

        assert!(store.is_installed(&name));
        assert_eq!(store.installed().len(), 1);
        assert_eq!(
            store.read_receipt(&name).unwrap().version.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn link_and_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let bin_name = PackageName::parse("foo").unwrap();
        let package_binary = PackagePath::parse("foo-bin").unwrap();
        let pkgdir = store.package_dir(&name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        store.write_receipt(&name, &receipt("foo")).unwrap();
        store.link_bin(&name, &package_binary, &bin_name).unwrap();

        assert!(store.bin_path(&name).is_some());
        store.remove(&name).unwrap();
        assert!(!store.is_installed(&name));
        assert!(!store.bin_dir().join("foo").exists());
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
        store.write_receipt(&name, &receipt("foo")).unwrap();
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
    fn failed_activation_restores_previous_package() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PackageStore::at(tmp.path());
        let name = PackageName::parse("foo").unwrap();
        let target = store.package_dir(&name);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old"), b"old").unwrap();

        let missing = store.staging_dir().join("missing");
        assert!(store.activate_package(&name, &missing).is_err());
        assert_eq!(std::fs::read(target.join("old")).unwrap(), b"old");
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
