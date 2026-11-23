use std::path::PathBuf;

use bevy_app::{App, Plugin, PreStartup};
use bevy_ecs::prelude::{Commands, Query, *};
use vmux_profile::mcp_credentials::{McpCredentialAccess, McpCredentialStorage};
use vmux_profile::{
    NativeRelocation, Profile, ProfilePaths, ProjectsDirectory, SessionEnvironment,
};

use super::migration::{MigrationBarrierSet, ProfileMigrationComplete};
use super::{ActiveProfile, McpCredentials, ProfileDirectories, ProjectsRoot, ProjectsRootFailure};

pub(super) struct BootstrapPlugin;

impl Plugin for BootstrapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreStartup,
            (
                seed_default,
                read_display_name,
                resolve_label,
                publish_active,
            )
                .chain()
                .after(MigrationBarrierSet),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub struct ProfileBootstrapSpec {
    pub profile: Profile,
    pub paths: ProfilePaths,
    pub test: bool,
    pub projects_root: PathBuf,
}

impl ProfileBootstrapSpec {
    pub fn current() -> Self {
        let profile = Profile::current();
        let paths = ProfilePaths::current();
        let projects_root = paths.projects();
        Self {
            profile,
            paths,
            test: SessionEnvironment::is_test(),
            projects_root,
        }
    }

    fn display_name_path(&self) -> PathBuf {
        self.paths
            .shared_data()
            .join("profiles")
            .join(self.profile.id())
            .join("display_name")
    }

    fn read_configured_name(&self) -> Option<String> {
        if self.test {
            return None;
        }
        NativeRelocation::read_display_name(&self.display_name_path())
            .ok()
            .flatten()
    }

    fn fallback_label(&self) -> String {
        let mut characters = self.profile.id().chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => "Personal".to_string(),
        }
    }
}

#[derive(Component)]
struct BootstrapPending;

#[derive(Component)]
struct ObservedDisplayName(Option<String>);

#[derive(Component)]
struct ResolvedProfileLabel(String);

fn seed_default(existing: Query<Entity, With<ProfileBootstrapSpec>>, mut commands: Commands) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((ProfileBootstrapSpec::current(), BootstrapPending));
}

fn read_display_name(
    completed: Query<Entity, With<ProfileMigrationComplete>>,
    specs: Query<(Entity, &ProfileBootstrapSpec), Without<ObservedDisplayName>>,
    mut commands: Commands,
) {
    if completed.is_empty() {
        return;
    }
    for (entity, spec) in &specs {
        let observed = spec.read_configured_name();
        commands
            .entity(entity)
            .remove::<BootstrapPending>()
            .insert(ObservedDisplayName(observed));
    }
}

fn resolve_label(
    specs: Query<
        (Entity, &ProfileBootstrapSpec, &ObservedDisplayName),
        Without<ResolvedProfileLabel>,
    >,
    mut commands: Commands,
) {
    for (entity, spec, observed) in &specs {
        let label = if !spec.test
            && let Some(name) = observed.0.as_deref().map(str::trim)
            && !name.is_empty()
        {
            name.to_string()
        } else {
            spec.fallback_label()
        };
        commands.entity(entity).insert(ResolvedProfileLabel(label));
    }
}

fn publish_active(
    specs: Query<(Entity, &ProfileBootstrapSpec, &ResolvedProfileLabel), Without<ActiveProfile>>,
    mut commands: Commands,
) {
    for (entity, spec, label) in &specs {
        let credentials = McpCredentials {
            access: McpCredentialAccess::default(),
            storage: McpCredentialStorage::at(spec.profile.clone(), spec.paths.clone()),
        };
        let mut active = commands.spawn((
            Name::new(label.0.clone()),
            ActiveProfile(spec.profile.clone()),
            ProfileDirectories(spec.paths.clone()),
            credentials,
        ));
        match ProjectsDirectory::ensure_at(spec.projects_root.clone()) {
            Ok(directory) => {
                active.insert(ProjectsRoot(directory));
            }
            Err(error) => {
                active.insert(ProjectsRootFailure(error));
            }
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{MigrationRefusal, Product, ProfileMigrationSpec, ProfilePlugin};

    struct ProfileFixture {
        temp: tempfile::TempDir,
    }

    impl ProfileFixture {
        fn new() -> Self {
            Self {
                temp: tempfile::tempdir().unwrap(),
            }
        }

        fn home(&self) -> PathBuf {
            self.temp.path().join("home")
        }

        fn managed_data(&self) -> PathBuf {
            self.temp.path().join("data/Vmux")
        }

        fn data(&self) -> PathBuf {
            self.managed_data().join("dev")
        }

        fn projects(&self) -> PathBuf {
            self.temp.path().join("projects")
        }

        fn run(&self, profile: &str, migration_disabled: bool, test_label: bool) -> App {
            let mut app = App::new();
            let paths = ProfilePaths::at(Profile::named(profile), self.data());
            app.add_plugins(ProfilePlugin);
            app.world_mut().spawn(ProfileMigrationSpec {
                home: self.home(),
                data: self.data(),
                managed_data: self.managed_data(),
                disabled: migration_disabled,
            });
            app.world_mut().spawn(ProfileBootstrapSpec {
                profile: Profile::named(profile),
                paths,
                test: test_label,
                projects_root: self.projects(),
            });
            app.update();
            app
        }

        fn refusals(app: &mut App) -> Vec<MigrationRefusal> {
            app.world_mut()
                .query::<&MigrationRefusal>()
                .iter(app.world())
                .cloned()
                .collect()
        }
    }

    #[test]
    fn migration_relocates_nested_spaces_recording_managed_dirs_and_profiles_in_order() {
        let fixture = ProfileFixture::new();
        let config = fixture.home().join(Product::config_name());
        std::fs::create_dir_all(config.join("profiles/personal/spaces/shared")).unwrap();
        std::fs::write(
            config.join("profiles/personal/spaces/shared/space.ron"),
            b"from-personal",
        )
        .unwrap();
        std::fs::create_dir_all(config.join("spaces/shared")).unwrap();
        std::fs::write(config.join("spaces/shared/space.ron"), b"from-legacy-root").unwrap();
        std::fs::create_dir_all(config.join("spaces/extra")).unwrap();
        std::fs::write(config.join("spaces/extra/space.ron"), b"from-extra").unwrap();
        std::fs::create_dir_all(config.join("recording")).unwrap();
        std::fs::write(config.join("recording/a.mp4"), b"video").unwrap();
        std::fs::create_dir_all(config.join("agents")).unwrap();
        std::fs::write(config.join("agents/registry.json"), b"{}").unwrap();
        std::fs::create_dir_all(config.join("profiles/gregor/recording")).unwrap();
        std::fs::write(config.join("profiles/gregor/recording/b.mp4"), b"other").unwrap();

        let mut app = fixture.run("personal", false, false);

        assert_eq!(
            app.world_mut()
                .query::<&ProfileMigrationComplete>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            std::fs::read(fixture.data().join("spaces/shared/space.ron")).unwrap(),
            b"from-personal"
        );
        assert_eq!(
            std::fs::read(config.join("spaces/shared/space.ron")).unwrap(),
            b"from-legacy-root"
        );
        assert_eq!(
            std::fs::read(fixture.data().join("spaces/extra/space.ron")).unwrap(),
            b"from-extra"
        );
        assert_eq!(
            std::fs::read(fixture.data().join("profiles/personal/recording/a.mp4")).unwrap(),
            b"video"
        );
        assert_eq!(
            std::fs::read(fixture.managed_data().join("agents/registry.json")).unwrap(),
            b"{}"
        );
        assert_eq!(
            std::fs::read(fixture.data().join("profiles/gregor/recording/b.mp4")).unwrap(),
            b"other"
        );
        assert!(!config.join("profiles").exists());
        assert!(!ProfileFixture::refusals(&mut app).is_empty());

        let mut restart = fixture.run("personal", false, false);
        assert_eq!(
            restart
                .world_mut()
                .query::<&ProfileMigrationComplete>()
                .iter(restart.world())
                .count(),
            1
        );
        assert_eq!(
            std::fs::read(fixture.data().join("spaces/shared/space.ron")).unwrap(),
            b"from-personal"
        );
    }

    #[test]
    fn relocation_refuses_occupied_target_without_overwriting() {
        let fixture = ProfileFixture::new();
        let source = fixture.temp.path().join("source.txt");
        let target = fixture.temp.path().join("target.txt");
        std::fs::write(&source, b"incoming").unwrap();
        std::fs::write(&target, b"existing").unwrap();

        let result = NativeRelocation::relocate_no_replace(&source, &target);

        assert!(result.is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"incoming");
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_relocated_without_traversal_and_never_pruned() {
        let fixture = ProfileFixture::new();
        let config = fixture.home().join(Product::config_name());
        let external = fixture.temp.path().join("external");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(external.join("secret.txt"), b"stay").unwrap();

        std::fs::create_dir_all(config.join("spaces")).unwrap();
        std::os::unix::fs::symlink(&external, config.join("spaces/linked")).unwrap();
        std::os::unix::fs::symlink(
            fixture.temp.path().join("does-not-exist"),
            config.join("spaces/dangling"),
        )
        .unwrap();

        let space_link_target = fixture.temp.path().join("empty-linked-dir");
        std::fs::create_dir_all(&space_link_target).unwrap();
        std::fs::create_dir_all(fixture.data().join("spaces")).unwrap();
        std::os::unix::fs::symlink(
            &space_link_target,
            fixture.data().join("spaces/symlink-keep"),
        )
        .unwrap();

        let _app = fixture.run("personal", false, false);

        assert!(
            fixture
                .data()
                .join("spaces/linked")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fixture
                .data()
                .join("spaces/dangling")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(external.join("secret.txt")).unwrap(), b"stay");
        assert!(
            fixture
                .data()
                .join("spaces/symlink-keep")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(space_link_target.is_dir());
    }

    #[test]
    fn pruning_removes_empty_space_directories_and_preserves_nonempty() {
        let fixture = ProfileFixture::new();
        std::fs::create_dir_all(fixture.data().join("spaces/org/empty")).unwrap();
        std::fs::create_dir_all(fixture.data().join("spaces/solo")).unwrap();
        std::fs::create_dir_all(fixture.data().join("spaces/keep")).unwrap();
        std::fs::write(fixture.data().join("spaces/keep/f.txt"), b"x").unwrap();

        let _app = fixture.run("personal", false, false);

        assert!(!fixture.data().join("spaces/org/empty").exists());
        assert!(!fixture.data().join("spaces/org").exists());
        assert!(!fixture.data().join("spaces/solo").exists());
        assert!(fixture.data().join("spaces/keep").is_dir());
        assert_eq!(
            std::fs::read(fixture.data().join("spaces/keep/f.txt")).unwrap(),
            b"x"
        );
    }

    #[test]
    fn disabled_migration_leaves_legacy_sources_untouched_while_preserving_display_name() {
        let fixture = ProfileFixture::new();
        let config = fixture.home().join(Product::config_name());
        std::fs::create_dir_all(config.join("spaces/legacy")).unwrap();
        std::fs::write(config.join("spaces/legacy/space.ron"), b"legacy").unwrap();
        std::fs::create_dir_all(fixture.data().join("profiles/personal")).unwrap();
        std::fs::write(
            fixture.data().join("profiles/personal/display_name"),
            "  Junichi \n",
        )
        .unwrap();

        let mut app = fixture.run("personal", true, false);

        assert!(config.join("spaces/legacy/space.ron").exists());
        assert!(!fixture.data().join("spaces/legacy").exists());
        assert!(ProfileFixture::refusals(&mut app).is_empty());

        let (active, name, directories, projects) = app
            .world_mut()
            .query::<(
                &ActiveProfile,
                &Name,
                &ProfileDirectories,
                Option<&ProjectsRoot>,
            )>()
            .single(app.world())
            .unwrap();
        assert_eq!(active.0, Profile::named("personal"));
        assert_eq!(name.as_str(), "Junichi");
        assert_eq!(directories.0.shared_data(), fixture.data());
        assert!(projects.is_some());
    }

    #[test]
    fn test_label_mode_falls_back_to_capitalized_profile_id_and_tolerates_migration_refusal() {
        let fixture = ProfileFixture::new();
        let config = fixture.home().join(Product::config_name());
        std::fs::create_dir_all(config.join("recording")).unwrap();
        std::fs::write(config.join("recording/clip.mp4"), b"new").unwrap();
        std::fs::create_dir_all(fixture.data().join("profiles/personal")).unwrap();
        std::fs::write(
            fixture.data().join("profiles/personal/recording"),
            b"blocking-file",
        )
        .unwrap();
        std::fs::write(
            fixture.data().join("profiles/personal/display_name"),
            "Custom Name",
        )
        .unwrap();

        let mut app = fixture.run("personal", false, true);

        assert!(!ProfileFixture::refusals(&mut app).is_empty());
        assert_eq!(
            std::fs::read(fixture.data().join("profiles/personal/recording")).unwrap(),
            b"blocking-file"
        );
        assert!(config.join("recording/clip.mp4").exists());
        let name = app
            .world_mut()
            .query_filtered::<&Name, With<ActiveProfile>>()
            .single(app.world())
            .unwrap();
        assert_eq!(name.as_str(), "Personal");
    }
}
