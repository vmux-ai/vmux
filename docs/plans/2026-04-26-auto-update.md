# Auto Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add silent automatic updates to Vmux -- check GitHub Releases on startup + hourly, download full signed .app bundle in background, swap on next launch.

**Architecture:** Background thread polls GitHub API, downloads tarball to staging dir. On next launch, `main()` detects staged update, atomically swaps .app bundle, and re-execs. Uses `std::thread::spawn` + `mpsc::channel` (matching existing settings hot-reload pattern). No async runtime needed.

**Tech Stack:** `reqwest` (blocking, rustls-tls), `semver`, `serde_json`, `flate2`, `tar`, `sha2`

**Spec:** `docs/specs/2026-04-26-auto-update-design.md`

---

## File Structure

```
crates/vmux_desktop/
├── Cargo.toml              # Add new dependencies
├── src/
│   ├── main.rs             # Add pre-startup apply_staged_update() call
│   ├── lib.rs              # Add updater module declaration + UpdatePlugin
│   ├── settings.rs         # Add auto_update field
│   ├── settings.ron        # Add auto_update: true
│   ├── updater.rs          # Module root: UpdatePlugin, timer, thread spawn
│   └── updater/
│       ├── github.rs       # GitHub Releases API client
│       ├── download.rs     # Streaming download + SHA-256 verification
│       ├── stage.rs        # Extract tar.gz to staging dir, write meta
│       └── apply.rs        # Swap .app bundle, re-exec
.github/workflows/
└── release.yml             # Add app bundle tarball + sha256 upload
```

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml`

- [ ] **Step 1: Add new dependencies**

Add after the existing `notify` dependency:

```toml
reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }
semver = "1"
serde_json = "1"
flate2 = "1"
tar = "0.4"
sha2 = "0.10"
```

- [ ] **Step 2: Verify it compiles**

Run: `cd crates/vmux_desktop && cargo check`
Expected: compiles without errors (new deps are unused so far -- warnings are fine)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/Cargo.toml
git commit -m "chore: add auto-update dependencies"
```

---

### Task 2: Add auto_update setting

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`
- Modify: `crates/vmux_desktop/src/settings.ron`

- [ ] **Step 1: Add auto_update field to AppSettings**

In `settings.rs`, add the field to `AppSettings`:

```rust
#[derive(Clone, Debug, Deserialize, Resource)]
pub struct AppSettings {
    pub browser: BrowserSettings,
    pub layout: LayoutSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub terminal: Option<TerminalSettings>,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}
```

Add the default function (after the existing default functions, e.g. near `default_leader`):

```rust
fn default_auto_update() -> bool {
    true
}
```

- [ ] **Step 2: Add auto_update to embedded settings.ron**

Add `auto_update: true,` at the end of the top-level tuple, before the closing `)`:

```ron
    // ... existing terminal settings ...
    )),
    auto_update: true,
)
```

- [ ] **Step 3: Verify it compiles and parses**

Run: `cd crates/vmux_desktop && cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs crates/vmux_desktop/src/settings.ron
git commit -m "feat: add auto_update setting to AppSettings"
```

---

### Task 3: GitHub API client

**Files:**
- Create: `crates/vmux_desktop/src/updater/github.rs`

- [ ] **Step 1: Create updater directory**

```bash
mkdir -p crates/vmux_desktop/src/updater
```

- [ ] **Step 2: Write github.rs**

```rust
use semver::Version;
use serde::Deserialize;

const GITHUB_API_URL: &str =
    "https://api.github.com/repos/vmux-ai/vmux/releases/latest";
const APP_TARBALL_PATTERN: &str = "aarch64-apple-darwin.app.tar.gz";
const SHA256_SUFFIX: &str = ".sha256";

#[derive(Debug)]
pub struct ReleaseInfo {
    pub version: Version,
    pub tarball_url: String,
    pub sha256_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Fetch the latest release from GitHub and return info if it's newer than `current`.
pub fn check_for_update(current: &Version) -> Result<Option<ReleaseInfo>, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{current}"))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(Error::Http)?;

    let resp = client.get(GITHUB_API_URL).send().map_err(Error::Http)?;

    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(Error::RateLimited);
    }

    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let release: GitHubRelease = resp.json().map_err(Error::Http)?;

    let tag = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    let latest = Version::parse(tag).map_err(Error::SemVer)?;

    if latest <= *current {
        return Ok(None);
    }

    let tarball = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(APP_TARBALL_PATTERN) && !a.name.ends_with(SHA256_SUFFIX))
        .ok_or(Error::MissingAsset("app tarball"))?;

    let sha256 = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(&format!("{APP_TARBALL_PATTERN}{SHA256_SUFFIX}")))
        .ok_or(Error::MissingAsset("sha256"))?;

    Ok(Some(ReleaseInfo {
        version: latest,
        tarball_url: tarball.browser_download_url.clone(),
        sha256_url: sha256.browser_download_url.clone(),
    }))
}

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    RateLimited,
    SemVer(semver::Error),
    MissingAsset(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "HTTP error: {e}"),
            Error::HttpStatus(s) => write!(f, "HTTP status: {s}"),
            Error::RateLimited => write!(f, "GitHub API rate limited"),
            Error::SemVer(e) => write!(f, "version parse error: {e}"),
            Error::MissingAsset(name) => write!(f, "missing release asset: {name}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_release_json() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "assets": [
                {
                    "name": "Vmux-v0.2.0-aarch64-apple-darwin.app.tar.gz",
                    "browser_download_url": "https://example.com/app.tar.gz"
                },
                {
                    "name": "Vmux-v0.2.0-aarch64-apple-darwin.app.tar.gz.sha256",
                    "browser_download_url": "https://example.com/app.tar.gz.sha256"
                },
                {
                    "name": "vmux-v0.2.0-aarch64-apple-darwin.tar.gz",
                    "browser_download_url": "https://example.com/binary.tar.gz"
                }
            ]
        }"#;

        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        let tag = release.tag_name.strip_prefix('v').unwrap();
        let version = Version::parse(tag).unwrap();
        assert_eq!(version, Version::new(0, 2, 0));

        let tarball = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(APP_TARBALL_PATTERN) && !a.name.ends_with(SHA256_SUFFIX))
            .unwrap();
        assert_eq!(tarball.browser_download_url, "https://example.com/app.tar.gz");

        let sha = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(&format!("{APP_TARBALL_PATTERN}{SHA256_SUFFIX}")))
            .unwrap();
        assert_eq!(sha.browser_download_url, "https://example.com/app.tar.gz.sha256");
    }

    #[test]
    fn current_version_is_latest_returns_none() {
        // Simulate: latest == current -> no update
        let current = Version::new(0, 2, 0);
        let latest = Version::parse("0.2.0").unwrap();
        assert!(latest <= current);
    }

    #[test]
    fn newer_version_detected() {
        let current = Version::new(0, 1, 0);
        let latest = Version::parse("0.2.0").unwrap();
        assert!(latest > current);
    }
}
```

- [ ] **Step 3: Verify tests pass**

Run: `cd crates/vmux_desktop && cargo test updater::github`
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/updater/github.rs
git commit -m "feat: add GitHub Releases API client for auto-update"
```

---

### Task 4: Download + checksum verification

**Files:**
- Create: `crates/vmux_desktop/src/updater/download.rs`

- [ ] **Step 1: Write download.rs**

```rust
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Download a file from `url` to `dest`, streaming to disk.
/// Returns the SHA-256 hex digest of the downloaded file.
pub fn download_file(url: &str, dest: &Path) -> Result<String, Error> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(Error::Http)?;

    let mut resp = client.get(url).send().map_err(Error::Http)?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let mut file = std::fs::File::create(dest).map_err(Error::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = resp.read(&mut buf).map_err(Error::Io)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(Error::Io)?;
        hasher.update(&buf[..n]);
    }

    file.flush().map_err(Error::Io)?;
    drop(file);

    let hash = format!("{:x}", hasher.finalize());
    Ok(hash)
}

/// Fetch the expected SHA-256 hash from a `.sha256` URL.
/// The file is expected to contain just the hex digest (no filename).
pub fn fetch_expected_sha256(url: &str) -> Result<String, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vmux/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(Error::Http)?;

    let resp = client.get(url).send().map_err(Error::Http)?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status()));
    }

    let text = resp.text().map_err(Error::Http)?;
    Ok(text.trim().to_string())
}

/// Download tarball + sha256, verify checksum.
/// Returns (path to downloaded tarball, verified SHA-256 hex digest).
pub fn download_and_verify(
    tarball_url: &str,
    sha256_url: &str,
    download_dir: &Path,
) -> Result<(PathBuf, String), Error> {
    // Clean up any previous partial download
    if download_dir.exists() {
        std::fs::remove_dir_all(download_dir).map_err(Error::Io)?;
    }

    let tarball_path = download_dir.join("update.tar.gz");

    let expected_hash = fetch_expected_sha256(sha256_url)?;
    let actual_hash = download_file(tarball_url, &tarball_path)?;

    if actual_hash != expected_hash {
        let _ = std::fs::remove_dir_all(download_dir);
        return Err(Error::ChecksumMismatch {
            expected: expected_hash,
            actual: actual_hash,
        });
    }

    Ok((tarball_path, actual_hash))
}

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    HttpStatus(reqwest::StatusCode),
    Io(std::io::Error),
    ChecksumMismatch { expected: String, actual: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "download HTTP error: {e}"),
            Error::HttpStatus(s) => write!(f, "download HTTP status: {s}"),
            Error::Io(e) => write!(f, "download I/O error: {e}"),
            Error::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn sha256_of_known_data() {
        let data = b"hello vmux update";
        let hash = format!("{:x}", Sha256::digest(data));
        assert_eq!(hash.len(), 64);
        // Verify deterministic
        let hash2 = format!("{:x}", Sha256::digest(data));
        assert_eq!(hash, hash2);
    }
}
```

- [ ] **Step 2: Verify tests pass**

Run: `cd crates/vmux_desktop && cargo test updater::download`
Expected: 1 test passes

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/updater/download.rs
git commit -m "feat: add download + checksum verification for auto-update"
```

---

### Task 5: Stage extracted bundle

**Files:**
- Create: `crates/vmux_desktop/src/updater/stage.rs`

- [ ] **Step 1: Write stage.rs**

```rust
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
pub fn stage_update(
    tarball_path: &Path,
    version: &str,
    sha256: &str,
) -> Result<(), Error> {
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

/// Simple ISO-8601 timestamp without pulling in chrono.
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
```

- [ ] **Step 2: Verify tests pass**

Run: `cd crates/vmux_desktop && cargo test updater::stage`
Expected: 3 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/updater/stage.rs
git commit -m "feat: add update staging and extraction for auto-update"
```

---

### Task 6: Apply update on launch

**Files:**
- Create: `crates/vmux_desktop/src/updater/apply.rs`

- [ ] **Step 1: Write apply.rs**

```rust
use semver::Version;
use std::path::{Path, PathBuf};

use super::stage;

/// Derive the .app bundle path from the current executable.
/// e.g. /Applications/Vmux.app/Contents/MacOS/Vmux -> /Applications/Vmux.app
pub fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Walk up: Vmux -> MacOS -> Contents -> Vmux.app
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;

    // Sanity check: must end with .app
    if bundle.extension().and_then(|e| e.to_str()) == Some("app") {
        Some(bundle.to_path_buf())
    } else {
        None
    }
}

/// Check for a staged update and apply it if valid.
/// This runs in main() before Bevy starts.
/// Returns `true` if the process should re-exec (update was applied).
pub fn apply_staged_update() -> bool {
    if !stage::has_staged_update() {
        return false;
    }

    let meta = match stage::read_meta() {
        Some(m) => m,
        None => return false,
    };

    let current_version = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let staged_version = match Version::parse(&meta.version) {
        Ok(v) => v,
        Err(_) => {
            stage::cleanup();
            return false;
        }
    };

    // Don't apply if staged version is same or older
    if staged_version <= current_version {
        stage::cleanup();
        return false;
    }

    let bundle_path = match current_app_bundle() {
        Some(p) => p,
        None => {
            eprintln!("[updater] cannot determine .app bundle path, skipping update");
            return false;
        }
    };

    let staged_app = match stage::staged_dir() {
        Some(d) => d.join("Vmux.app"),
        None => return false,
    };

    if !staged_app.exists() {
        stage::cleanup();
        return false;
    }

    eprintln!(
        "[updater] applying update v{} -> v{}",
        current_version, staged_version
    );

    match swap_app_bundle(&bundle_path, &staged_app) {
        Ok(()) => {
            stage::cleanup();
            true
        }
        Err(e) => {
            eprintln!("[updater] failed to apply update: {e}");
            false
        }
    }
}

/// Atomically swap the current .app bundle with the staged one.
fn swap_app_bundle(current: &Path, staged: &Path) -> Result<(), Error> {
    let old = current.with_extension("app.old");

    // Step 1: rename current -> .old
    std::fs::rename(current, &old).map_err(|e| Error::Rename {
        from: current.clone(),
        to: old.clone(),
        source: e,
    })?;

    // Step 2: move staged -> current
    if let Err(e) = std::fs::rename(staged, current) {
        // Rollback: restore old
        eprintln!("[updater] move staged -> current failed, rolling back");
        let _ = std::fs::rename(&old, current);
        return Err(Error::Rename {
            from: staged.clone(),
            to: current.clone(),
            source: e,
        });
    }

    // Step 3: remove old (non-fatal)
    if let Err(e) = std::fs::remove_dir_all(&old) {
        eprintln!("[updater] warning: failed to remove old bundle: {e}");
    }

    Ok(())
}

/// Re-exec the current binary (replaces the current process).
pub fn re_exec() -> ! {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().expect("cannot determine current executable");
    let args: Vec<String> = std::env::args().collect();
    let err = std::process::Command::new(&exe).args(&args[1..]).exec();
    // exec() only returns on error
    eprintln!("[updater] re-exec failed: {err}");
    std::process::exit(1);
}

#[derive(Debug)]
pub enum Error {
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Rename { from, to, source } => {
                write!(
                    f,
                    "failed to rename {} -> {}: {}",
                    from.display(),
                    to.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_bundle_path_from_exe() {
        // Simulate: /Applications/Vmux.app/Contents/MacOS/Vmux
        let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");
        let macos = exe.parent().unwrap();
        let contents = macos.parent().unwrap();
        let bundle = contents.parent().unwrap();
        assert_eq!(bundle, PathBuf::from("/Applications/Vmux.app").as_path());
        assert_eq!(bundle.extension().unwrap(), "app");
    }

    #[test]
    fn non_app_bundle_returns_none_from_logic() {
        // Simulate: /usr/local/bin/vmux (not in .app bundle)
        let exe = PathBuf::from("/usr/local/bin/vmux");
        let macos = exe.parent();
        let contents = macos.and_then(|p| p.parent());
        let bundle = contents.and_then(|p| p.parent());
        // bundle would be Some("/"), which doesn't end in .app
        if let Some(b) = bundle {
            assert_ne!(b.extension().and_then(|e| e.to_str()), Some("app"));
        }
    }
}
```

- [ ] **Step 2: Verify tests pass**

Run: `cd crates/vmux_desktop && cargo test updater::apply`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/updater/apply.rs
git commit -m "feat: add update application (swap + re-exec) for auto-update"
```

---

### Task 7: UpdatePlugin and Bevy integration

**Files:**
- Create: `crates/vmux_desktop/src/updater.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: `crates/vmux_desktop/src/main.rs`

- [ ] **Step 1: Write updater.rs (module root + plugin)**

```rust
mod apply;
mod download;
mod github;
mod stage;

use bevy::prelude::*;
use std::sync::{mpsc, Arc, Mutex};

use crate::settings::AppSettings;

pub use apply::{apply_staged_update, re_exec};

const INITIAL_DELAY_SECS: f32 = 5.0;
const POLL_INTERVAL_SECS: f32 = 3600.0; // 1 hour

pub struct UpdatePlugin;

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_update_checker)
            .add_systems(Update, poll_update_result);
    }
}

#[derive(Resource)]
struct UpdateChecker {
    rx: Arc<Mutex<mpsc::Receiver<UpdateResult>>>,
    tx: mpsc::Sender<UpdateResult>,
    /// Timer controlling when the next check fires.
    timer: Timer,
    /// Whether the initial delay has elapsed.
    started: bool,
    /// Set to true once an update has been staged.
    done: bool,
    /// Whether a background check is currently running.
    in_flight: bool,
}

enum UpdateResult {
    NoUpdate,
    Staged { version: String },
    Failed(String),
}

fn init_update_checker(mut commands: Commands) {
    let (tx, rx) = mpsc::channel();

    commands.insert_resource(UpdateChecker {
        rx: Arc::new(Mutex::new(rx)),
        tx,
        timer: Timer::from_seconds(INITIAL_DELAY_SECS, TimerMode::Once),
        started: false,
        done: false,
        in_flight: false,
    });
}

fn poll_update_result(
    mut checker: ResMut<UpdateChecker>,
    settings: Res<AppSettings>,
    time: Res<Time>,
) {
    if checker.done {
        return;
    }

    // Drain results from background thread
    if let Ok(rx) = checker.rx.lock() {
        while let Ok(result) = rx.try_recv() {
            checker.in_flight = false;
            match result {
                UpdateResult::NoUpdate => {
                    bevy::log::debug!("no update available");
                }
                UpdateResult::Staged { version } => {
                    bevy::log::info!("update v{version} staged, will apply on next launch");
                    checker.done = true;
                    return;
                }
                UpdateResult::Failed(e) => {
                    bevy::log::debug!("update check failed: {e}");
                }
            }
        }
    }

    if !settings.auto_update {
        return;
    }

    // If a staged update already exists, don't check
    if stage::has_staged_update() {
        checker.done = true;
        bevy::log::debug!("staged update already present, skipping check");
        return;
    }

    // Don't fire another check while one is in progress
    if checker.in_flight {
        return;
    }

    checker.timer.tick(time.delta());

    if !checker.timer.just_finished() {
        return;
    }

    if !checker.started {
        checker.started = true;
        // Switch to repeating poll interval
        checker.timer = Timer::from_seconds(POLL_INTERVAL_SECS, TimerMode::Repeating);
    }

    // Fire check on background thread using the persistent channel
    let tx = checker.tx.clone();
    checker.in_flight = true;

    std::thread::spawn(move || {
        let result = run_update_check();
        let _ = tx.send(result);
    });
}

fn run_update_check() -> UpdateResult {
    let current = match semver::Version::parse(env!("CARGO_PKG_VERSION")) {
        Ok(v) => v,
        Err(e) => return UpdateResult::Failed(format!("bad current version: {e}")),
    };

    let release = match github::check_for_update(&current) {
        Ok(Some(r)) => r,
        Ok(None) => return UpdateResult::NoUpdate,
        Err(e) => return UpdateResult::Failed(format!("check failed: {e}")),
    };

    let download_dir = match stage::downloading_dir() {
        Some(d) => d,
        None => return UpdateResult::Failed("no cache dir".to_string()),
    };

    let (tarball, sha256) = match download::download_and_verify(
        &release.tarball_url,
        &release.sha256_url,
        &download_dir,
    ) {
        Ok(result) => result,
        Err(e) => return UpdateResult::Failed(format!("download failed: {e}")),
    };

    let version_str = release.version.to_string();
    if let Err(e) = stage::stage_update(&tarball, &version_str, &sha256) {
        return UpdateResult::Failed(format!("staging failed: {e}"));
    }

    UpdateResult::Staged {
        version: version_str,
    }
}
```

- [ ] **Step 2: Add module declaration to lib.rs**

In `crates/vmux_desktop/src/lib.rs`, add the module declaration alongside the existing ones:

```rust
mod updater;
```

Add the import alongside the existing plugin imports:

```rust
use updater::UpdatePlugin;
```

Add `UpdatePlugin` to the plugin registration. Add it in the second `.add_plugins((...))` call:

```rust
        .add_plugins((
            TerminalInputPlugin,
            PersistencePlugin,
            ProfilePlugin,
            LayoutPlugin,
            UpdatePlugin,
        ));
```

- [ ] **Step 3: Add pre-startup update apply to main.rs**

In `crates/vmux_desktop/src/lib.rs`, change the module visibility to `pub` (needed because `main.rs` is a separate binary crate):

```rust
pub mod updater;
```

In `crates/vmux_desktop/src/main.rs`, add the import and pre-startup check. The full updated `main.rs`:

```rust
use bevy::prelude::*;
use vmux_desktop::VmuxPlugin;

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    // Apply any staged update before starting the app
    if vmux_desktop::updater::apply_staged_update() {
        vmux_desktop::updater::re_exec();
    }

    println!(
        // ... existing banner (unchanged) ...
    );

    // ... rest of main unchanged ...
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd crates/vmux_desktop && cargo check`
Expected: compiles without errors

- [ ] **Step 5: Run all updater tests**

Run: `cd crates/vmux_desktop && cargo test updater`
Expected: all tests pass (github: 3, download: 1, stage: 3, apply: 2)

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/updater.rs crates/vmux_desktop/src/updater/ \
  crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/main.rs
git commit -m "feat: wire up UpdatePlugin and pre-launch update apply"
```

---

### Task 8: CI pipeline changes

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Add app bundle tarball step**

After the "Create binary tarball" step and before "Create GitHub Release", add:

```yaml
      - name: Create app bundle tarball
        run: |
          cd target/release
          tar czf "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" Vmux.app
          shasum -a 256 "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" \
            | awk '{print $1}' > "Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz.sha256"
```

- [ ] **Step 2: Update GitHub Release upload**

Change the `gh release create` command to include the new artifacts:

```yaml
          gh release create "v${VERSION}" \
            --title "Vmux v${VERSION}" \
            --generate-notes \
            "$DMG_PATH" \
            "target/release/vmux-v${VERSION}-aarch64-apple-darwin.tar.gz" \
            "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz" \
            "target/release/Vmux-v${VERSION}-aarch64-apple-darwin.app.tar.gz.sha256"
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add signed app bundle tarball + sha256 to releases"
```

---

### Task 9: Build verification

- [ ] **Step 1: Full cargo check**

Run: `cargo check -p vmux_desktop`
Expected: compiles without errors

- [ ] **Step 2: Run all tests**

Run: `cargo test -p vmux_desktop -- updater`
Expected: all 9 tests pass

- [ ] **Step 3: Verify no warnings in updater code**

Run: `cargo clippy -p vmux_desktop -- -W clippy::all 2>&1 | grep updater`
Expected: no warnings from updater modules

- [ ] **Step 4: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "chore: fixups from build verification"
```

---

## Manual Test Plan (post-implementation)

These cannot be automated but should be verified before merging:

1. **Settings opt-out:** Set `auto_update: false` in `settings.ron`, verify no HTTP requests are made (check debug logs).
2. **Staged update detection:** Manually create `~/Library/Caches/ai.vmux.desktop/updates/staged/Vmux.app` with a fake `update-meta.json` pointing to a newer version. Launch app. Verify it attempts the swap (will fail since the staged app isn't real, but the logic path is exercised).
3. **Version comparison:** Verify that when the latest GitHub release matches the current version, no download occurs.
4. **End-to-end (requires a release):** After a release with the new CI artifacts, verify the full flow: check -> download -> stage -> restart -> apply.
