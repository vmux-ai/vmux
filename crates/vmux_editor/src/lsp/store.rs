use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Default managed install root: `~/.vmux/lsp`.
pub fn default_root() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".vmux")
        .join("lsp")
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

/// Install record written to `packages/<name>/vmux-receipt.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub name: String,
    pub version: Option<String>,
    pub source_id: String,
    /// link name -> file (relative to the package dir) to symlink under `bin/`.
    pub bin: BTreeMap<String, String>,
}

fn receipt_path(root: &Path, name: &str) -> PathBuf {
    packages_dir(root).join(name).join("vmux-receipt.json")
}

pub fn write_receipt(root: &Path, r: &Receipt) -> io::Result<()> {
    let dir = packages_dir(root).join(&r.name);
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(r)?;
    std::fs::write(receipt_path(root, &r.name), json)
}

pub fn read_receipt(root: &Path, name: &str) -> Option<Receipt> {
    let bytes = std::fs::read(receipt_path(root, name)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn installed(root: &Path) -> BTreeMap<String, Receipt> {
    let mut out = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(packages_dir(root)) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str()
                && let Some(r) = read_receipt(root, name)
            {
                out.insert(name.to_string(), r);
            }
        }
    }
    out
}

pub fn is_installed(root: &Path, name: &str) -> bool {
    receipt_path(root, name).is_file()
}

/// Symlink `bin/<link_name>` → `../packages/<name>/<file>`.
pub fn link_bin(root: &Path, name: &str, file: &str, link_name: &str) -> io::Result<()> {
    let bin = bin_dir(root);
    std::fs::create_dir_all(&bin)?;
    let link = bin.join(link_name);
    let target = packages_dir(root).join(name).join(file);
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &link)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::copy(&target, &link)?;
    }
    Ok(())
}

/// Absolute path to a package's primary bin link, if installed.
pub fn bin_path(root: &Path, name: &str) -> Option<PathBuf> {
    let r = read_receipt(root, name)?;
    let link_name = r.bin.keys().next()?;
    let p = bin_dir(root).join(link_name);
    p.exists().then_some(p)
}

pub fn remove(root: &Path, name: &str) -> io::Result<()> {
    if let Some(r) = read_receipt(root, name) {
        for link_name in r.bin.keys() {
            let _ = std::fs::remove_file(bin_dir(root).join(link_name));
        }
    }
    let dir = packages_dir(root).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// How a server command resolves: managed install, system PATH, or missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Managed(PathBuf),
    OnPath,
    Missing,
}

/// `PATH` for spawned servers: the managed `bin/` first (so managed sub-tools and
/// sibling servers resolve), then the current process `PATH`.
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
        // create the package payload + receipt
        let pkgdir = packages_dir(root).join("foo");
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"#!/bin/sh\n").unwrap();
        write_receipt(root, &receipt("foo")).unwrap();

        assert!(is_installed(root, "foo"));
        assert_eq!(installed(root).len(), 1);
        assert_eq!(read_receipt(root, "foo").unwrap().version.as_deref(), Some("1.0"));
    }

    #[test]
    fn link_and_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let pkgdir = packages_dir(root).join("foo");
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        write_receipt(root, &receipt("foo")).unwrap();
        link_bin(root, "foo", "foo-bin", "foo").unwrap();

        assert!(bin_path(root, "foo").is_some());
        remove(root, "foo").unwrap();
        assert!(!is_installed(root, "foo"));
        assert!(!bin_dir(root).join("foo").exists());
    }

    #[test]
    fn resolution_prefers_managed_then_path_then_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // managed
        let pkgdir = packages_dir(root).join("foo");
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("foo-bin"), b"x").unwrap();
        write_receipt(root, &receipt("foo")).unwrap();
        link_bin(root, "foo", "foo-bin", "foo").unwrap();
        assert!(matches!(resolved_command(root, "foo"), Resolution::Managed(_)));
        // on PATH (cargo exists everywhere this runs)
        assert_eq!(resolved_command(root, "cargo"), Resolution::OnPath);
        // missing
        assert_eq!(
            resolved_command(root, "definitely-not-real-zzz"),
            Resolution::Missing
        );
    }
}
