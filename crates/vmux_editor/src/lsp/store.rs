use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
#[path = "store.test.rs"]
mod tests;
