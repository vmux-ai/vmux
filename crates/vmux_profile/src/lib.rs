use std::path::PathBuf;

#[cfg(not(web))]
pub mod tools;
#[cfg(not(web))]
pub mod vault;

pub const fn build_profile() -> &'static str {
    env!("VMUX_BUILD_PROFILE")
}

pub const fn git_hash() -> &'static str {
    env!("VMUX_GIT_HASH")
}

pub fn active_profile_name() -> String {
    sanitize_profile(&std::env::var("VMUX_PROFILE").unwrap_or_default())
}

pub fn is_test_session() -> bool {
    matches!(
        std::env::var("VMUX_TEST").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

pub fn sanitize_profile(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "personal".to_string()
    } else {
        trimmed.to_string()
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Personal".to_string(),
    }
}

fn display_name_path() -> PathBuf {
    profile_dir().join("display_name")
}

fn display_name_from(configured: Option<&str>, id: &str, is_test: bool) -> String {
    if !is_test && let Some(name) = configured {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    capitalize_first(id)
}

pub fn display_name() -> String {
    let configured = std::fs::read_to_string(display_name_path()).ok();
    display_name_from(
        configured.as_deref(),
        &active_profile_name(),
        is_test_session(),
    )
}

pub fn set_display_name(name: &str) -> std::io::Result<()> {
    let path = display_name_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, name.trim())
}

fn data_dir_suffix_for(profile: &str) -> PathBuf {
    match profile {
        "release" | "local" => PathBuf::from("Vmux"),
        other => PathBuf::from("Vmux").join(other),
    }
}

fn data_dir_suffix() -> PathBuf {
    data_dir_suffix_for(build_profile())
}

pub fn shared_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(data_dir_suffix())
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join(data_dir_suffix())
    }
}

pub fn application_data_dir() -> PathBuf {
    let data = shared_data_dir();
    match build_profile() {
        "release" | "local" => data,
        _ => data.parent().map(PathBuf::from).unwrap_or(data),
    }
}

/// User config directory: `~/.vmux`. Holds `settings.ron` (and per-build
/// overrides), separate from the profile-isolated [`shared_data_dir`].
pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    home.join(".vmux")
}

/// Default root for repositories and local projects managed through vmux.
pub fn workspace_dir() -> PathBuf {
    config_dir().join("workspace")
}

/// Default output directory for screenshots and screen recordings below the
/// active runtime profile. Overridable via the
/// `recording.output_dir` setting.
fn recording_dir_for(data: &std::path::Path, profile: &str) -> PathBuf {
    data.join("profiles").join(profile).join("recording")
}

pub fn recording_dir() -> PathBuf {
    recording_dir_for(&shared_data_dir(), &active_profile_name())
}

/// Per-build config subdir, or `None` for the shared (release) settings.
fn config_suffix() -> Option<&'static str> {
    match build_profile() {
        "release" | "local" => None,
        other => Some(other),
    }
}

fn settings_candidates_in(base: &std::path::Path, suffix: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(suffix) = suffix {
        candidates.push(base.join(suffix).join("settings.ron"));
    }
    candidates.push(base.join("settings.ron"));
    candidates
}

/// Settings files in priority order: the per-build override first (e.g.
/// `~/.vmux/dev/settings.ron`), then the shared `~/.vmux/settings.ron`.
pub fn settings_path_candidates() -> Vec<PathBuf> {
    settings_candidates_in(&config_dir(), config_suffix())
}

/// The settings file to read/write: the first candidate that exists, falling
/// back to the shared `~/.vmux/settings.ron` when none exist yet.
pub fn settings_path() -> PathBuf {
    let candidates = settings_path_candidates();
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| {
            candidates
                .last()
                .cloned()
                .expect("settings candidates always include the shared path")
        })
}

pub fn profile_dir() -> PathBuf {
    shared_data_dir()
        .join("profiles")
        .join(active_profile_name())
}

pub fn session_path() -> PathBuf {
    profile_dir().join("session.ron")
}

pub fn cef_cache_path() -> Option<String> {
    profile_dir().to_str().map(|s| s.to_owned())
}

/// CEF command-line switches selecting how cookies and passwords are encrypted
/// at rest.
///
/// On macOS the encryption key lives in the login Keychain under the shared,
/// framework-default `Chromium Safe Storage` item (CEF exposes no way to rename
/// it), and access is gated by the requesting binary's code-signing identity.
/// All interactive builds — `dev`, `local`, and `release` — use the real
/// Keychain (no switches) so saved credentials stay securely encrypted.
/// Persistence across updates relies on a stable signing identity: Developer-ID
/// for `release`/`local`, and the reused self-signed `Vmux Dev` certificate that
/// `make dev` applies. Both yield a designated requirement that survives
/// rebuilds, so access sticks after a one-time "Always Allow" per identity.
///
/// Automated test sessions (`VMUX_TEST`) instead pass `use-mock-keychain`, which
/// derives the key from a constant. Those runs are often headless (no one to
/// approve the Keychain prompt) and use throwaway, frequently ad-hoc-signed
/// profiles whose changing identity would otherwise churn the ACL of the shared
/// item real logins depend on. Weak at-rest encryption is irrelevant for
/// disposable test data.
pub fn cef_keychain_switches() -> &'static [&'static str] {
    cef_keychain_switches_for(is_test_session())
}

fn cef_keychain_switches_for(is_test_session: bool) -> &'static [&'static str] {
    if is_test_session {
        &["use-mock-keychain"]
    } else {
        &[]
    }
}

fn store_dir_for(base: &std::path::Path, _profile: &str) -> PathBuf {
    base.to_path_buf()
}

pub fn store_dir() -> PathBuf {
    let dir = store_dir_for(&shared_data_dir(), &active_profile_name());
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn managed_dir(name: &str) -> PathBuf {
    application_data_dir().join(name)
}

pub fn agents_dir() -> PathBuf {
    managed_dir("agents")
}

pub fn extensions_dir() -> PathBuf {
    managed_dir("extensions")
}

pub fn lsp_dir() -> PathBuf {
    managed_dir("lsp")
}

fn spaces_root_for(data: &std::path::Path, _profile: &str) -> PathBuf {
    data.join("spaces")
}

#[cfg(test)]
fn space_dir_path(data: &std::path::Path, profile: &str, space_id: &str) -> PathBuf {
    spaces_root_for(data, profile).join(space_id)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn collect_subdirs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            collect_subdirs(&path, out);
            out.push(path);
        }
    }
}

fn prune_empty_legacy_space_dirs_in(data: &std::path::Path) {
    let root = spaces_root_for(data, "personal");
    if root
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return;
    }
    let mut dirs = Vec::new();
    collect_subdirs(&root, &mut dirs);
    for dir in dirs {
        if is_empty_dir(&dir) {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    if is_empty_dir(&root) {
        let _ = std::fs::remove_dir(root);
    }
}

fn migrate_dir(legacy: &std::path::Path, target: &std::path::Path) {
    let Ok(legacy_metadata) = legacy.symlink_metadata() else {
        return;
    };
    if target.symlink_metadata().is_err() {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(legacy, target);
        return;
    }
    if !legacy_metadata.file_type().is_dir()
        || !target
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
    for entry in entries.flatten() {
        migrate_dir(&entry.path(), &target.join(entry.file_name()));
    }
    let _ = std::fs::remove_dir(legacy);
}

fn migrate_legacy_personal_layout_in(
    home: &std::path::Path,
    data: &std::path::Path,
    managed_data: &std::path::Path,
) {
    let config = home.join(".vmux");
    migrate_dir(
        &config.join("profiles").join("personal").join("spaces"),
        &spaces_root_for(data, "personal"),
    );
    migrate_dir(&config.join("spaces"), &spaces_root_for(data, "personal"));
    migrate_dir(
        &config.join("recording"),
        &recording_dir_for(data, "personal"),
    );
    for name in ["agents", "extensions", "lsp"] {
        migrate_dir(&config.join(name), &managed_data.join(name));
    }
    if let Ok(profiles) = std::fs::read_dir(config.join("profiles")) {
        for profile in profiles.flatten() {
            if profile
                .file_type()
                .is_ok_and(|file_type| file_type.is_dir())
            {
                migrate_dir(
                    &profile.path(),
                    &data.join("profiles").join(profile.file_name()),
                );
            }
        }
    }
    let _ = std::fs::remove_dir(config.join("profiles"));
}

/// Relocate generated profile and managed-package data out of the Vault and
/// remove empty directories from the retired filesystem-backed spaces layout.
/// Skipped for test sessions.
pub fn migrate_legacy_personal_layout() {
    if is_test_session() {
        return;
    }
    let home = home_dir();
    let data = shared_data_dir();
    let managed_data = application_data_dir();
    migrate_legacy_personal_layout_in(&home, &data, &managed_data);
    prune_empty_legacy_space_dirs_in(&data);
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
