use std::collections::BTreeMap;
use std::fs::Metadata;
use std::path::PathBuf;

use crate::Result;

pub const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone)]
pub struct Policy {
    pub minimum: u64,
    pub warning: u64,
    pub cache_limit: u64,
    pub incremental_limit: u64,
    pub deps_limit: u64,
    pub local_keep: usize,
}

impl Policy {
    pub fn configured(environment: &crate::environment::Environment) -> Result<Self> {
        let minimum = environment
            .get("VMUX_BUILD_STORAGE_MIN_GIB")
            .unwrap_or(std::ffi::OsStr::new("20"));
        let warning = environment
            .get("VMUX_BUILD_STORAGE_WARN_GIB")
            .unwrap_or(std::ffi::OsStr::new("50"));
        let cache = environment
            .get("VMUX_BUILD_STORAGE_CACHE_GIB")
            .unwrap_or(std::ffi::OsStr::new("50"));
        let keep = environment
            .get("VMUX_BUILD_STORAGE_LOCAL_KEEP")
            .unwrap_or(std::ffi::OsStr::new("3"));
        let incremental = environment
            .get("VMUX_BUILD_STORAGE_INCREMENTAL_GIB")
            .unwrap_or(std::ffi::OsStr::new("8"));
        let deps = environment
            .get("VMUX_BUILD_STORAGE_DEPS_GIB")
            .unwrap_or(std::ffi::OsStr::new("16"));
        Self::parse(
            minimum.to_str().ok_or("Invalid storage minimum")?,
            warning.to_str().ok_or("Invalid storage warning")?,
            cache.to_str().ok_or("Invalid cache budget")?,
            keep.to_str().ok_or("Invalid artifact retention")?,
            incremental.to_str().ok_or("Invalid incremental budget")?,
            deps.to_str().ok_or("Invalid deps budget")?,
        )
    }

    pub fn check(&self, free: u64) -> Result<()> {
        if free < self.minimum {
            return Err(format!("Insufficient build space: need {} GiB free; stop unused apps/builds and run vmux cache clean --apply", self.minimum as f64 / GIB as f64).into());
        }
        Ok(())
    }

    pub(crate) fn prunable_deps(
        &self,
        entries: &[(PathBuf, Metadata)],
    ) -> Vec<(PathBuf, Metadata)> {
        let mut selected = Vec::new();
        for (path, metadata) in entries {
            if Self::is_runtime_library(path) {
                continue;
            }
            selected.push((path.clone(), metadata.clone()));
        }
        selected
    }

    pub(crate) fn stale_deps(
        &self,
        entries: &[(PathBuf, Metadata)],
        usage: u64,
    ) -> (Vec<(PathBuf, Metadata)>, u64) {
        if usage <= self.deps_limit {
            return (Vec::new(), usage);
        }
        let mut by_stem = BTreeMap::new();
        for (path, metadata) in entries {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some((stem, hash)) = Self::hashed_artifact(name) else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let protected = Self::is_runtime_library(path);
            let hashes = by_stem.entry(stem.to_owned()).or_insert_with(BTreeMap::new);
            let entry = hashes
                .entry(hash.to_owned())
                .or_insert_with(|| (modified, 0u64, false, Vec::new()));
            if modified > entry.0 {
                entry.0 = modified;
            }
            entry.1 = entry.1.saturating_add(metadata.len());
            entry.2 |= protected;
            entry.3.push((path.clone(), metadata.clone()));
        }
        let mut superseded = Vec::new();
        for (_, mut hashes) in by_stem {
            if hashes.len() < 2 {
                continue;
            }
            let mut newest = None;
            for (hash, (modified, _, _, _)) in &hashes {
                if newest.as_ref().is_none_or(|(best_time, best_hash)| {
                    (*modified, hash) > (*best_time, *best_hash)
                }) {
                    newest = Some((*modified, hash));
                }
            }
            let Some((_, newest_hash)) = newest else {
                continue;
            };
            let newest_hash = newest_hash.clone();
            hashes.remove(&newest_hash);
            for (_, (modified, size, protected, files)) in hashes {
                if !protected {
                    superseded.push((modified, size, files));
                }
            }
        }
        superseded.sort_by_key(|(modified, _, _)| *modified);
        let mut remaining = usage;
        let mut selected = Vec::new();
        for (_, size, files) in superseded {
            if remaining <= self.deps_limit {
                break;
            }
            selected.extend(files);
            remaining = remaining.saturating_sub(size);
        }
        (selected, remaining)
    }

    fn is_runtime_library(path: &std::path::Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == "dylib" || extension == "so")
    }

    fn hashed_artifact(name: &str) -> Option<(&str, &str)> {
        let base = name.split('.').next()?;
        let (stem, hash) = base.rsplit_once('-')?;
        if stem.is_empty()
            || hash.len() != 16
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        Some((stem, hash))
    }

    #[cfg(test)]
    pub fn environment() -> Result<Self> {
        let minimum = std::env::var("VMUX_BUILD_STORAGE_MIN_GIB").unwrap_or_else(|_| "20".into());
        let warning = std::env::var("VMUX_BUILD_STORAGE_WARN_GIB").unwrap_or_else(|_| "50".into());
        let cache = std::env::var("VMUX_BUILD_STORAGE_CACHE_GIB").unwrap_or_else(|_| "50".into());
        let keep = std::env::var("VMUX_BUILD_STORAGE_LOCAL_KEEP").unwrap_or_else(|_| "3".into());
        let incremental =
            std::env::var("VMUX_BUILD_STORAGE_INCREMENTAL_GIB").unwrap_or_else(|_| "8".into());
        let deps = std::env::var("VMUX_BUILD_STORAGE_DEPS_GIB").unwrap_or_else(|_| "16".into());
        Self::parse(&minimum, &warning, &cache, &keep, &incremental, &deps)
    }

    fn parse(
        minimum: &str,
        warning: &str,
        cache: &str,
        keep: &str,
        incremental: &str,
        deps: &str,
    ) -> Result<Self> {
        let minimum = Self::bytes(minimum)?;
        let warning = Self::bytes(warning)?;
        let cache_limit = Self::bytes(cache)?;
        let incremental_limit = Self::bytes(incremental)?;
        let deps_limit = Self::bytes(deps)?;
        let local_keep = keep.parse()?;
        if local_keep < 1 || warning < minimum {
            return Err("Keep at least one local app and set warning >= minimum".into());
        }
        Ok(Self {
            minimum,
            warning,
            cache_limit,
            incremental_limit,
            deps_limit,
            local_keep,
        })
    }

    fn bytes(value: &str) -> Result<u64> {
        let value: f64 = value.parse()?;
        let bytes = value * GIB as f64;
        if !value.is_finite() || value < 0.0 || bytes >= u64::MAX as f64 {
            return Err("Storage thresholds must be finite nonnegative numbers".into());
        }
        Ok(bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::Policy;

    #[test]
    fn rejects_invalid_storage_policies() {
        for values in [
            ["nan", "50", "50", "3"],
            ["20", "50", "-1", "3"],
            ["60", "50", "50", "3"],
            ["20", "50", "50", "0"],
            ["inf", "inf", "50", "3"],
        ] {
            assert!(Policy::parse(values[0], values[1], values[2], values[3], "8", "16").is_err());
        }
        for invalid in ["-1", "NaN", "inf", "invalid"] {
            assert!(Policy::parse("20", "50", "50", "3", invalid, "16").is_err());
            assert!(Policy::parse("20", "50", "50", "3", "8", invalid).is_err());
        }
    }

    #[test]
    fn warning_threshold_does_not_refuse_a_build() {
        let policy = Policy::parse("20", "50", "50", "3", "8", "16").unwrap();
        assert!(policy.check(10 * super::GIB).is_err());
        assert!(policy.check(30 * super::GIB).is_ok());
    }
}
