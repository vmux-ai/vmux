use std::path::{Path, PathBuf};

use crate::{Profile, ProfilePaths, SessionEnvironment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRecord {
    pub profile: Profile,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct ProfileStore {
    data: PathBuf,
    active: Profile,
    test: bool,
}

pub struct ProfileMigration {
    home: PathBuf,
    data: PathBuf,
    managed_data: PathBuf,
    test: bool,
}

impl ProfileMigration {
    pub fn current() -> Self {
        let paths = ProfilePaths::current();
        Self {
            home: Self::home(),
            data: paths.shared_data(),
            managed_data: paths.application_data(),
            test: SessionEnvironment::is_test(),
        }
    }

    pub fn run(&self) {
        if self.test {
            return;
        }
        self.migrate_legacy_layout();
        self.prune_empty_space_directories();
    }

    fn home() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn migrate_legacy_layout(&self) {
        let config = self.home.join(".vmux");
        Self::migrate_directory(&config.join("profiles/personal/spaces"), &self.spaces());
        Self::migrate_directory(&config.join("spaces"), &self.spaces());
        Self::migrate_directory(&config.join("recording"), &self.recording("personal"));
        for name in ["agents", "extensions", "lsp"] {
            Self::migrate_directory(&config.join(name), &self.managed_data.join(name));
        }
        if let Ok(profiles) = std::fs::read_dir(config.join("profiles")) {
            for profile in profiles.flatten() {
                if profile
                    .file_type()
                    .is_ok_and(|file_type| file_type.is_dir())
                {
                    Self::migrate_directory(
                        &profile.path(),
                        &self.data.join("profiles").join(profile.file_name()),
                    );
                }
            }
        }
        let _ = std::fs::remove_dir(config.join("profiles"));
    }

    fn migrate_directory(legacy: &Path, target: &Path) {
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
            Self::migrate_directory(&entry.path(), &target.join(entry.file_name()));
        }
        let _ = std::fs::remove_dir(legacy);
    }

    fn prune_empty_space_directories(&self) {
        let root = self.spaces();
        if root
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return;
        }
        let mut directories = Vec::new();
        Self::collect_directories(&root, &mut directories);
        for directory in directories {
            if Self::is_empty_directory(&directory) {
                let _ = std::fs::remove_dir(&directory);
            }
        }
        if Self::is_empty_directory(&root) {
            let _ = std::fs::remove_dir(root);
        }
    }

    fn collect_directories(directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                Self::collect_directories(&path, found);
                found.push(path);
            }
        }
    }

    fn is_empty_directory(path: &Path) -> bool {
        std::fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    }

    fn spaces(&self) -> PathBuf {
        self.data.join("spaces")
    }

    #[cfg(test)]
    fn space(&self, id: &str) -> PathBuf {
        self.spaces().join(id)
    }

    fn recording(&self, profile: &str) -> PathBuf {
        self.data.join("profiles").join(profile).join("recording")
    }
}

impl ProfileStore {
    pub fn current() -> Self {
        Self {
            data: ProfilePaths::current().shared_data(),
            active: Profile::current(),
            test: SessionEnvironment::is_test(),
        }
    }

    pub fn at(data: impl Into<PathBuf>, active: Profile, test: bool) -> Self {
        Self {
            data: data.into(),
            active,
            test,
        }
    }

    pub fn active(&self) -> &Profile {
        &self.active
    }

    pub fn active_label(&self) -> String {
        self.label(&self.active)
    }

    pub fn profiles(&self) -> Vec<ProfileRecord> {
        let root = self.data.join("profiles");
        let mut ids = std::collections::BTreeSet::new();
        ids.insert(self.active.id().to_string());
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    ids.insert(Profile::named(&entry.file_name().to_string_lossy()).into_id());
                }
            }
        }
        ids.into_iter()
            .map(|id| {
                let profile = Profile::named(&id);
                ProfileRecord {
                    name: self.label(&profile),
                    active: profile == self.active,
                    profile,
                }
            })
            .collect()
    }

    pub fn create(&self, name: &str) -> std::io::Result<ProfileRecord> {
        let name = Self::validated_name(name)?;
        let base = Profile::named(name).into_id();
        let root = self.data.join("profiles");
        let mut id = base.clone();
        for suffix in 2usize.. {
            if !root.join(&id).exists() {
                break;
            }
            id = format!("{base}-{suffix}");
        }
        let profile = Profile::named(&id);
        let directory = root.join(profile.id());
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join("display_name"), name)?;
        Ok(ProfileRecord {
            profile,
            name: name.to_string(),
            active: false,
        })
    }

    pub fn rename(&self, profile: &Profile, name: &str) -> std::io::Result<ProfileRecord> {
        let name = Self::validated_name(name)?;
        let path = self.name_path(profile);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, name)?;
        Ok(ProfileRecord {
            profile: profile.clone(),
            name: name.to_string(),
            active: profile == &self.active,
        })
    }

    pub fn exists(&self, profile: &Profile) -> bool {
        profile == &self.active
            || self
                .name_path(profile)
                .parent()
                .is_some_and(|path| path.is_dir())
    }

    pub fn label(&self, profile: &Profile) -> String {
        let configured = std::fs::read_to_string(self.name_path(profile)).ok();
        if !self.test
            && let Some(name) = configured.as_deref().map(str::trim)
            && !name.is_empty()
        {
            return name.to_string();
        }
        Self::fallback_label(profile.id())
    }

    fn name_path(&self, profile: &Profile) -> PathBuf {
        self.data
            .join("profiles")
            .join(profile.id())
            .join("display_name")
    }

    fn validated_name(name: &str) -> std::io::Result<&str> {
        let name = name.trim();
        if name.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "profile name cannot be empty",
            ));
        }
        Ok(name)
    }

    fn fallback_label(id: &str) -> String {
        let mut characters = id.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => "Personal".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_includes_active_and_saved_profiles() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("profiles/work")).unwrap();
        std::fs::write(
            temp.path().join("profiles/work/display_name"),
            "Client Work",
        )
        .unwrap();
        let store = ProfileStore::at(temp.path(), Profile::named("personal"), false);

        let profiles = store.profiles();

        assert_eq!(
            profiles
                .iter()
                .map(|record| record.profile.id())
                .collect::<Vec<_>>(),
            ["personal", "work"]
        );
        assert_eq!(profiles[1].name, "Client Work");
    }

    #[test]
    fn creation_uses_a_unique_safe_id() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("profiles/client-work")).unwrap();
        let store = ProfileStore::at(temp.path(), Profile::named("personal"), false);

        let created = store.create("Client Work").unwrap();

        assert_eq!(created.profile.id(), "client-work-2");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("profiles/client-work-2/display_name"))
                .unwrap(),
            "Client Work"
        );
    }

    #[test]
    fn label_uses_config_or_capitalized_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = ProfileStore::at(temp.path(), Profile::named("personal"), false);
        assert_eq!(store.active_label(), "Personal");
        store.rename(store.active(), "Junichi").unwrap();
        assert_eq!(store.active_label(), "Junichi");
        let test_store = ProfileStore::at(temp.path(), Profile::named("personal"), true);
        assert_eq!(test_store.active_label(), "Personal");
    }

    fn migration(home: &Path, data: &Path, managed_data: &Path) -> ProfileMigration {
        ProfileMigration {
            home: home.to_path_buf(),
            data: data.to_path_buf(),
            managed_data: managed_data.to_path_buf(),
            test: false,
        }
    }

    #[test]
    fn migration_relocates_nested_spaces_and_recording() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let managed_data = home.join("data/Vmux");
        let data = managed_data.join("dev");
        let nested_space = home.join(".vmux/profiles/personal/spaces/space-1");
        std::fs::create_dir_all(&nested_space).unwrap();
        std::fs::write(nested_space.join("space.ron"), b"x").unwrap();
        let legacy_recording = home.join(".vmux/recording");
        std::fs::create_dir_all(&legacy_recording).unwrap();
        std::fs::write(legacy_recording.join("a.mp4"), b"y").unwrap();
        let agents = home.join(".vmux/agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(agents.join("registry.json"), b"{}").unwrap();
        let profile_recording = home.join(".vmux/profiles/gregor/recording");
        std::fs::create_dir_all(&profile_recording).unwrap();
        std::fs::write(profile_recording.join("b.mp4"), b"z").unwrap();

        migration(home, &data, &managed_data).migrate_legacy_layout();

        assert!(data.join("spaces/space-1/space.ron").exists());
        assert!(!home.join(".vmux/profiles/personal/spaces").exists());
        assert!(!legacy_recording.exists());
        assert!(data.join("profiles/personal/recording/a.mp4").exists());
        assert!(managed_data.join("agents/registry.json").exists());
        assert!(data.join("profiles/gregor/recording/b.mp4").exists());
        assert!(!home.join(".vmux/profiles").exists());
    }

    #[test]
    fn migration_keeps_existing_spaces() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let data = home.join("data/Vmux");
        std::fs::create_dir_all(home.join(".vmux/profiles/personal/spaces")).unwrap();
        let target = data.join("spaces");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();

        migration(home, &data, &data).migrate_legacy_layout();

        assert!(target.join("keep.txt").exists());
    }

    #[test]
    fn migration_prunes_empty_space_directories_and_preserves_files() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let data = home.join("data/Vmux");
        let migration = migration(home, &data, &data);
        std::fs::create_dir_all(migration.space("org/empty")).unwrap();
        std::fs::create_dir_all(migration.space("solo")).unwrap();
        std::fs::create_dir_all(migration.space("keep")).unwrap();
        std::fs::write(migration.space("keep/f.txt"), b"x").unwrap();

        migration.prune_empty_space_directories();

        assert!(!migration.space("org/empty").exists());
        assert!(!migration.space("org").exists());
        assert!(!migration.space("solo").exists());
        assert!(migration.space("keep").is_dir());
    }
}
