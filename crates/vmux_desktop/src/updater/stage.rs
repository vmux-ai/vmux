use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const UPDATE_DIR_NAME: &str = "updates";
const DOWNLOADING_DIR: &str = "downloading";
const STAGED_DIR: &str = "staged";
const META_FILE: &str = "update-meta.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMeta {
    pub version: String,
    pub sha256: String,
    pub timestamp: String,
}

/// Return the base update directory: ~/Library/Caches/ai.vmux.desktop/updates/
pub fn updates_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("ai", "vmux", "desktop")
        .map(|p| p.cache_dir().join(UPDATE_DIR_NAME))
}

pub fn downloading_dir() -> Option<PathBuf> {
    updates_dir().map(|p| p.join(DOWNLOADING_DIR))
}

pub fn staged_dir() -> Option<PathBuf> {
    updates_dir().map(|p| p.join(STAGED_DIR))
}

pub fn meta_path() -> Option<PathBuf> {
    updates_dir().map(|p| p.join(META_FILE))
}

/// Read update-meta.json if it exists.
pub fn read_meta() -> Option<UpdateMeta> {
    let path = meta_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Extract tarball to staging directory and write update-meta.json.
pub fn stage_update(tarball_path: &Path, version: &str, sha256: &str) -> Result<(), Error> {
    let staged = staged_dir().ok_or(Error::NoCacheDir)?;

    // Clean previous staged update
    if staged.exists() {
        std::fs::remove_dir_all(&staged).map_err(Error::Io)?;
    }
    std::fs::create_dir_all(&staged).map_err(Error::Io)?;

    // Extract tar.gz
    let file = std::fs::File::open(tarball_path).map_err(Error::Io)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&staged).map_err(Error::Io)?;

    // Verify Vmux.app exists in staged dir
    let app_bundle = staged.join("Vmux.app");
    if !app_bundle.exists() {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(Error::MissingAppBundle);
    }

    // Write meta
    let meta = UpdateMeta {
        version: version.to_string(),
        sha256: sha256.to_string(),
        timestamp: chrono_free_timestamp(),
    };
    let meta_path = meta_path().ok_or(Error::NoCacheDir)?;
    let json = serde_json::to_string_pretty(&meta).map_err(Error::Json)?;
    std::fs::write(&meta_path, json).map_err(Error::Io)?;

    // Clean up downloading dir
    if let Some(dl) = downloading_dir() {
        let _ = std::fs::remove_dir_all(dl);
    }

    Ok(())
}

/// Clean up all update state (staged + meta + downloading).
pub fn cleanup() {
    if let Some(dir) = updates_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Simple timestamp without pulling in chrono.
fn chrono_free_timestamp() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

/// Check if a staged update is ready to apply.
pub fn has_staged_update() -> bool {
    let Some(staged) = staged_dir() else {
        return false;
    };
    let Some(meta) = read_meta() else {
        return false;
    };
    staged.join("Vmux.app").exists() && !meta.version.is_empty()
}

#[derive(Debug)]
pub enum Error {
    NoCacheDir,
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingAppBundle,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoCacheDir => write!(f, "cannot determine cache directory"),
            Error::Io(e) => write!(f, "staging I/O error: {e}"),
            Error::Json(e) => write!(f, "meta JSON error: {e}"),
            Error::MissingAppBundle => write!(f, "Vmux.app not found in extracted archive"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_dir_is_in_caches() {
        let dir = updates_dir().unwrap();
        let path_str = dir.to_string_lossy();
        assert!(path_str.contains("ai.vmux.desktop"));
        assert!(path_str.ends_with("updates"));
    }

    #[test]
    fn meta_round_trip() {
        let meta = UpdateMeta {
            version: "0.2.0".to_string(),
            sha256: "abc123".to_string(),
            timestamp: "1234567890".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: UpdateMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "0.2.0");
        assert_eq!(parsed.sha256, "abc123");
    }

    #[test]
    fn chrono_free_timestamp_is_numeric() {
        let ts = chrono_free_timestamp();
        assert!(ts.parse::<u64>().is_ok());
    }
}
