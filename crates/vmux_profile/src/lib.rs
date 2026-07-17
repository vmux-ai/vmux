use std::path::PathBuf;

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
    config_dir()
        .join("profiles")
        .join(active_profile_name())
        .join("display_name")
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

/// User config directory: `~/.vmux`. Holds `settings.ron` (and per-build
/// overrides), separate from the profile-isolated [`shared_data_dir`].
pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    home.join(".vmux")
}

/// Default output directory for screenshots and screen recordings:
/// `~/.vmux/profiles/<profile>/recording`. Overridable via the
/// `recording.output_dir` setting.
fn recording_dir_for(config: &std::path::Path, profile: &str) -> PathBuf {
    config.join("profiles").join(profile).join("recording")
}

pub fn recording_dir() -> PathBuf {
    recording_dir_for(&config_dir(), &active_profile_name())
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

pub fn default_space_dir() -> PathBuf {
    space_dir("space-1")
}

fn spaces_root_for(home: &std::path::Path, _profile: &str) -> PathBuf {
    home.join(".vmux").join("spaces")
}

fn space_dir_path(home: &std::path::Path, profile: &str, space_id: &str) -> PathBuf {
    spaces_root_for(home, profile).join(space_id)
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

/// Remove `dir` and its now-empty ancestors, stopping at (and never removing)
/// `root`.
fn prune_empty_dirs_up(mut dir: PathBuf, root: &std::path::Path) {
    while dir.as_path() != root && dir.starts_with(root) {
        if !is_empty_dir(&dir) || std::fs::remove_dir(&dir).is_err() {
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
}

pub fn space_dir(space_id: &str) -> PathBuf {
    let dir = space_dir_path(&home_dir(), &active_profile_name(), space_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn rename_space_dir_in(home: &std::path::Path, profile: &str, old_id: &str, new_id: &str) {
    if old_id == new_id {
        return;
    }
    let old = space_dir_path(home, profile, old_id);
    let new = space_dir_path(home, profile, new_id);
    if old == new {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if old.exists() {
        if is_empty_dir(&new) {
            let _ = std::fs::remove_dir(&new);
        }
        if std::fs::rename(&old, &new).is_ok()
            && let Some(parent) = old.parent()
        {
            prune_empty_dirs_up(parent.to_path_buf(), &spaces_root_for(home, profile));
        }
    } else if !new.exists() {
        let _ = std::fs::create_dir_all(&new);
    }
}

pub fn rename_space_dir(old_id: &str, new_id: &str) {
    rename_space_dir_in(&home_dir(), &active_profile_name(), old_id, new_id);
}

fn collect_subdirs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_subdirs(&path, out);
            out.push(path);
        }
    }
}

/// Remove directories under `~/.vmux/spaces/` that no longer match a live space
/// id and are empty. Folders that contain files (or back a live space) are kept.
fn prune_orphan_space_dirs_in(
    home: &std::path::Path,
    profile: &str,
    live: &std::collections::HashSet<String>,
) {
    let root = spaces_root_for(home, profile);
    let mut dirs = Vec::new();
    collect_subdirs(&root, &mut dirs);
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for dir in dirs {
        let Ok(rel) = dir.strip_prefix(&root) else {
            continue;
        };
        let rel_id = rel.to_string_lossy().replace('\\', "/");
        if live.contains(&rel_id) {
            continue;
        }
        if is_empty_dir(&dir) {
            let _ = std::fs::remove_dir(&dir);
        }
    }
}

pub fn prune_orphan_space_dirs(live: &std::collections::HashSet<String>) {
    prune_orphan_space_dirs_in(&home_dir(), &active_profile_name(), live);
}

fn migrate_dir(legacy: &std::path::Path, target: &std::path::Path) {
    if !legacy.exists() || target.exists() {
        return;
    }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::rename(legacy, target);
}

fn migrate_legacy_personal_layout_in(home: &std::path::Path) {
    let config = home.join(".vmux");
    migrate_dir(
        &config.join("profiles").join("personal").join("spaces"),
        &spaces_root_for(home, "personal"),
    );
    migrate_dir(
        &config.join("recording"),
        &recording_dir_for(&config, "personal"),
    );
}

/// Relocate the default profile's layout to the profile-agnostic dirs and undo
/// #145's per-profile spaces nesting. Skipped for test sessions.
pub fn migrate_legacy_personal_layout() {
    if is_test_session() {
        return;
    }
    migrate_legacy_personal_layout_in(&home_dir());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_dir_is_nested_under_profile() {
        assert_eq!(
            recording_dir_for(std::path::Path::new("/home/u/.vmux"), "personal"),
            PathBuf::from("/home/u/.vmux/profiles/personal/recording")
        );
    }

    #[test]
    fn recording_dir_test_profile_is_nested() {
        assert_eq!(
            recording_dir_for(std::path::Path::new("/home/u/.vmux"), "test"),
            PathBuf::from("/home/u/.vmux/profiles/test/recording")
        );
    }

    #[test]
    fn sanitize_profile_keeps_safe_and_defaults_empty() {
        assert_eq!(sanitize_profile("test"), "test");
        assert_eq!(sanitize_profile("Test"), "test");
        assert_eq!(sanitize_profile(""), "personal");
        assert_eq!(sanitize_profile("  "), "personal");
        assert_eq!(sanitize_profile("a/b"), "a-b");
        assert_eq!(sanitize_profile("../evil"), "evil");
    }

    #[test]
    fn store_dir_is_profile_agnostic_base() {
        let base = std::path::Path::new("/data/Vmux/dev");
        assert_eq!(
            store_dir_for(base, "personal"),
            PathBuf::from("/data/Vmux/dev")
        );
        assert_eq!(
            store_dir_for(base, "gregor"),
            PathBuf::from("/data/Vmux/dev")
        );
    }

    #[test]
    fn is_test_session_reads_env() {
        let prev = std::env::var("VMUX_TEST").ok();
        unsafe { std::env::set_var("VMUX_TEST", "1") };
        assert!(is_test_session());
        unsafe { std::env::remove_var("VMUX_TEST") };
        assert!(!is_test_session());
        if let Some(p) = prev {
            unsafe { std::env::set_var("VMUX_TEST", p) };
        }
    }

    #[test]
    fn display_name_uses_config_or_capitalized_id() {
        assert_eq!(display_name_from(None, "personal", false), "Personal");
        assert_eq!(
            display_name_from(Some("Junichi"), "personal", false),
            "Junichi"
        );
        assert_eq!(
            display_name_from(Some("Junichi"), "personal", true),
            "Personal"
        );
        assert_eq!(display_name_from(Some("  "), "gregor", false), "Gregor");
    }

    #[test]
    fn spaces_root_is_profile_agnostic() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(
            spaces_root_for(home, "personal"),
            PathBuf::from("/home/u/.vmux/spaces")
        );
        assert_eq!(
            spaces_root_for(home, "gregor"),
            PathBuf::from("/home/u/.vmux/spaces")
        );
    }

    #[test]
    fn active_profile_name_reads_and_sanitizes_env() {
        let prev = std::env::var("VMUX_PROFILE").ok();
        unsafe { std::env::set_var("VMUX_PROFILE", "Test/X") };
        assert_eq!(active_profile_name(), "test-x");
        unsafe { std::env::remove_var("VMUX_PROFILE") };
        assert_eq!(active_profile_name(), "personal");
        if let Some(p) = prev {
            unsafe { std::env::set_var("VMUX_PROFILE", p) };
        }
    }

    #[test]
    fn data_dir_suffix_maps_each_profile() {
        assert_eq!(data_dir_suffix_for("release"), PathBuf::from("Vmux"));
        assert_eq!(data_dir_suffix_for("local"), PathBuf::from("Vmux"));
        assert_eq!(
            data_dir_suffix_for("dev"),
            PathBuf::from("Vmux").join("dev")
        );
        assert_eq!(
            data_dir_suffix_for("custom"),
            PathBuf::from("Vmux").join("custom"),
        );
    }

    #[test]
    fn local_and_release_share_one_space() {
        assert_eq!(data_dir_suffix_for("local"), data_dir_suffix_for("release"));
    }

    #[test]
    fn test_sessions_use_mock_keychain() {
        assert_eq!(
            cef_keychain_switches_for(true),
            ["use-mock-keychain"].as_slice()
        );
    }

    #[test]
    fn interactive_sessions_use_real_keychain() {
        assert!(cef_keychain_switches_for(false).is_empty());
    }

    #[test]
    fn dev_lives_under_the_release_space() {
        let release = data_dir_suffix_for("release");
        let dev = data_dir_suffix_for("dev");
        assert!(dev.starts_with(&release));
        assert_ne!(dev, release);
        assert_eq!(dev.file_name().unwrap(), "dev");
    }

    #[test]
    fn shared_data_dir_ends_with_profile_suffix() {
        assert!(shared_data_dir().ends_with(data_dir_suffix()));
    }

    #[test]
    fn space_dir_is_under_vmux_spaces() {
        assert_eq!(
            space_dir_path(std::path::Path::new("/home/u"), "personal", "work"),
            PathBuf::from("/home/u/.vmux/spaces/work")
        );
    }

    #[test]
    fn migrate_relocates_nested_spaces_and_recording() {
        let home = std::env::temp_dir().join(format!("vmux-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let nested_space = home
            .join(".vmux")
            .join("profiles")
            .join("personal")
            .join("spaces")
            .join("space-1");
        std::fs::create_dir_all(&nested_space).unwrap();
        std::fs::write(nested_space.join("space.ron"), b"x").unwrap();
        let legacy_rec = home.join(".vmux").join("recording");
        std::fs::create_dir_all(&legacy_rec).unwrap();
        std::fs::write(legacy_rec.join("a.mp4"), b"y").unwrap();

        migrate_legacy_personal_layout_in(&home);

        assert!(
            space_dir_path(&home, "personal", "space-1")
                .join("space.ron")
                .exists()
        );
        assert!(
            !home
                .join(".vmux")
                .join("profiles")
                .join("personal")
                .join("spaces")
                .exists()
        );
        assert!(!legacy_rec.exists());
        assert!(
            recording_dir_for(&home.join(".vmux"), "personal")
                .join("a.mp4")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_keeps_existing_agnostic_spaces() {
        let home = std::env::temp_dir().join(format!("vmux-migrate-noop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(
            home.join(".vmux")
                .join("profiles")
                .join("personal")
                .join("spaces"),
        )
        .unwrap();
        let target = spaces_root_for(&home, "personal");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();

        migrate_legacy_personal_layout_in(&home);

        assert!(target.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_space_dir_moves_existing_folder() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-mv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "personal", "old")).unwrap();
        rename_space_dir_in(&home, "personal", "old", "new");
        assert!(!space_dir_path(&home, "personal", "old").exists());
        assert!(space_dir_path(&home, "personal", "new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_space_dir_creates_folder_when_absent() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        rename_space_dir_in(&home, "personal", "old", "new");
        assert!(space_dir_path(&home, "personal", "new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_space_dir_creates_nested_path() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-nest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "personal", "old")).unwrap();
        rename_space_dir_in(&home, "personal", "old", "org/new");
        assert!(!space_dir_path(&home, "personal", "old").exists());
        assert!(space_dir_path(&home, "personal", "org/new").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn rename_prunes_empty_old_parent() {
        let home = std::env::temp_dir().join(format!("vmux-rndir-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "personal", "org/old")).unwrap();
        rename_space_dir_in(&home, "personal", "org/old", "elsewhere");
        assert!(space_dir_path(&home, "personal", "elsewhere").is_dir());
        assert!(!space_dir_path(&home, "personal", "org/old").exists());
        assert!(!space_dir_path(&home, "personal", "org").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn prune_removes_empty_orphans_keeps_live_and_files() {
        let home = std::env::temp_dir().join(format!("vmux-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&home, "personal", "org/live")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "personal", "org/orphan")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "personal", "solo")).unwrap();
        std::fs::create_dir_all(space_dir_path(&home, "personal", "keep")).unwrap();
        std::fs::write(
            space_dir_path(&home, "personal", "keep").join("f.txt"),
            b"x",
        )
        .unwrap();

        let live: std::collections::HashSet<String> =
            ["org/live".to_string()].into_iter().collect();
        prune_orphan_space_dirs_in(&home, "personal", &live);

        assert!(space_dir_path(&home, "personal", "org/live").is_dir());
        assert!(space_dir_path(&home, "personal", "org").is_dir());
        assert!(!space_dir_path(&home, "personal", "org/orphan").exists());
        assert!(!space_dir_path(&home, "personal", "solo").exists());
        assert!(space_dir_path(&home, "personal", "keep").is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }
    #[test]
    fn settings_live_in_dot_vmux_not_data_dir() {
        for candidate in settings_path_candidates() {
            assert!(candidate.starts_with(config_dir()));
            assert!(!candidate.starts_with(shared_data_dir()));
        }
    }

    #[test]
    fn settings_candidates_prefer_per_build_override_then_shared() {
        let base = PathBuf::from("/base");
        assert_eq!(
            settings_candidates_in(&base, None),
            vec![base.join("settings.ron")]
        );
        assert_eq!(
            settings_candidates_in(&base, Some("dev")),
            vec![
                base.join("dev").join("settings.ron"),
                base.join("settings.ron"),
            ]
        );
    }
}
