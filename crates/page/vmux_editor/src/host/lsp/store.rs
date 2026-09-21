use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lsp::package_path::{PackageName, PackagePath};

pub fn default_root() -> PathBuf {
    vmux_core::profile::lsp_dir()
}

pub fn bin_dir(root: &Path) -> PathBuf {
    root.join("bin")
}

pub fn packages_dir(root: &Path) -> PathBuf {
    root.join("packages")
}

pub fn staging_dir(root: &Path) -> PathBuf {
    root.join("staging")
}

pub fn registries_dir(root: &Path) -> PathBuf {
    root.join("registries")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub name: String,
    pub version: Option<String>,
    pub source_id: String,
    pub bin: BTreeMap<String, String>,
}

pub fn package_dir(root: &Path, name: &PackageName) -> PathBuf {
    packages_dir(root).join(name.as_str())
}

fn receipt_path(root: &Path, name: &PackageName) -> PathBuf {
    package_dir(root, name).join("vmux-receipt.json")
}

pub fn write_receipt(root: &Path, name: &PackageName, receipt: &Receipt) -> io::Result<()> {
    let dir = package_dir(root, name);
    std::fs::create_dir_all(&dir)?;
    write_receipt_in(&dir, receipt)
}

pub fn write_receipt_in(package_dir: &Path, receipt: &Receipt) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(receipt)?;
    vmux_path::AtomicFile::write(package_dir.join("vmux-receipt.json"), &json)
}

pub fn activate_package(root: &Path, name: &PackageName, staged: &Path) -> io::Result<()> {
    let packages = packages_dir(root);
    std::fs::create_dir_all(&packages)?;
    let target = package_dir(root, name);
    let backup =
        staging_dir(root).join(format!(".{}.{}.backup", name.as_str(), std::process::id()));
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

pub fn read_receipt(root: &Path, name: &PackageName) -> Option<Receipt> {
    let bytes = std::fs::read(receipt_path(root, name)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn installed(root: &Path) -> BTreeMap<String, Receipt> {
    let mut out = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(packages_dir(root)) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str()
                && let Ok(name) = PackageName::parse(name)
                && let Some(r) = read_receipt(root, &name)
            {
                out.insert(name.as_str().to_string(), r);
            }
        }
    }
    out
}

pub fn is_installed(root: &Path, name: &PackageName) -> bool {
    receipt_path(root, name).is_file()
}

pub fn link_bin(
    root: &Path,
    name: &PackageName,
    file: &PackagePath,
    link_name: &PackageName,
) -> io::Result<()> {
    let bin = bin_dir(root);
    std::fs::create_dir_all(&bin)?;
    let link = bin.join(link_name.as_str());
    let target = package_dir(root, name).join(file.as_path());
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

pub fn bin_path(root: &Path, name: &PackageName) -> Option<PathBuf> {
    let r = read_receipt(root, name)?;
    let link_name = r.bin.keys().next()?;
    let p = bin_dir(root).join(link_name);
    p.exists().then_some(p)
}

pub fn remove(root: &Path, name: &PackageName) -> io::Result<()> {
    if let Some(r) = read_receipt(root, name) {
        for link_name in r.bin.keys() {
            if let Ok(link_name) = PackageName::parse(link_name) {
                let _ = std::fs::remove_file(bin_dir(root).join(link_name.as_str()));
            }
        }
    }
    let dir = package_dir(root, name);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Managed(PathBuf),
    OnPath,
    Missing,
}

pub fn server_path_env(root: &Path) -> std::ffi::OsString {
    let mut parts: Vec<PathBuf> = vec![bin_dir(root)];
    if let Some(cur) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(parts).unwrap_or_default()
}

pub fn resolved_command(root: &Path, cmd: &str) -> Resolution {
    let managed = bin_dir(root).join(cmd);
    if managed.is_file() || managed.is_symlink() {
        return Resolution::Managed(managed);
    }
    if crate::lsp::registry::executable_on_path(cmd) {
        return Resolution::OnPath;
    }
    Resolution::Missing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(name: &str) -> Receipt {
        let mut bin = BTreeMap::new();
        bin.insert(name.to_string(), format!("{name}-bin"));
        Receipt {
            name: name.to_string(),
            version: Some("1.0".into()),
            source_id: "pkg:github/x/y@1.0".into(),
            bin,
        }
    }

    #[test]
    fn write_read_installed_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let name = PackageName::parse("foo").unwrap();
        let pkgdir = package_dir(root, &name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"#!/bin/sh\n").unwrap();
        write_receipt(root, &name, &receipt("foo")).unwrap();

        assert!(is_installed(root, &name));
        assert_eq!(installed(root).len(), 1);
        assert_eq!(
            read_receipt(root, &name).unwrap().version.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn link_and_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let name = PackageName::parse("foo").unwrap();
        let bin_name = PackageName::parse("foo").unwrap();
        let package_binary = PackagePath::parse("foo-bin").unwrap();
        let pkgdir = package_dir(root, &name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        write_receipt(root, &name, &receipt("foo")).unwrap();
        link_bin(root, &name, &package_binary, &bin_name).unwrap();

        assert!(bin_path(root, &name).is_some());
        remove(root, &name).unwrap();
        assert!(!is_installed(root, &name));
        assert!(!bin_dir(root).join("foo").exists());
    }

    #[test]
    fn resolution_prefers_managed_then_path_then_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let name = PackageName::parse("foo").unwrap();
        let bin_name = PackageName::parse("foo").unwrap();
        let package_binary = PackagePath::parse("foo-bin").unwrap();
        let pkgdir = package_dir(root, &name);
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        write_receipt(root, &name, &receipt("foo")).unwrap();
        link_bin(root, &name, &package_binary, &bin_name).unwrap();
        assert!(matches!(
            resolved_command(root, "foo"),
            Resolution::Managed(_)
        ));
        assert_eq!(resolved_command(root, "cargo"), Resolution::OnPath);
        assert_eq!(
            resolved_command(root, "definitely-not-real-zzz"),
            Resolution::Missing
        );
    }

    #[test]
    fn failed_activation_restores_previous_package() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let name = PackageName::parse("foo").unwrap();
        let target = package_dir(root, &name);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old"), b"old").unwrap();

        let missing = staging_dir(root).join("missing");
        assert!(activate_package(root, &name, &missing).is_err());
        assert_eq!(std::fs::read(target.join("old")).unwrap(), b"old");
    }
}
