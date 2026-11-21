use bevy::prelude::*;
use bevy::tasks::Task;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{ToolStore, ToolsManifest};
pub(crate) use file::{FileReadRequest, FileReadResult};
pub(crate) use read::{ReadRequest, ReadResult};
pub(crate) use write::{
    BrewfileTemplate, ManifestOnly, ManifestReceipt, WriteRequest, WriteResult,
};

mod file;
mod migration;
mod native;
mod read;
mod write;

pub(crate) struct StoragePlugin;

impl Plugin for StoragePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.add_plugins((
            file::FileReadPlugin,
            migration::MigrationPlugin,
            read::ReadPlugin,
            write::WritePlugin,
        ))
        .configure_sets(
            Update,
            (read::ReadSet, write::WriteSet).after(crate::ToolTaskSet::Start),
        )
        .add_systems(Update, admit.before(read::ReadSet).before(write::WriteSet))
        .add_systems(PostUpdate, wake)
        .add_systems(
            PostUpdate,
            (
                poll::<StoreLease>,
                poll::<native::Probe>,
                poll::<native::Moved>,
                poll::<native::ManifestBytes>,
                poll::<native::Brewfile>,
                poll::<native::ManifestWritten>,
                poll::<native::BrewfileWritten>,
                poll::<file::FileBytes>,
            ),
        );
    }
}

#[derive(Component, Clone)]
pub(crate) struct StoreLease {
    pub(crate) _lease: crate::manifest::Lease,
}

#[derive(Component)]
struct AcquiredLease;

#[derive(Component)]
struct Reading;

#[derive(Component)]
struct Writing;

#[derive(Component)]
struct RetainedHere;

#[derive(Component)]
struct Busy;

#[derive(Component)]
struct IoTask<T: Component>(Task<Result<T, String>>);

#[derive(Component)]
struct Failed<T: Component>(String, std::marker::PhantomData<fn() -> T>);

#[derive(Component)]
struct Failure(String);

fn admit(
    requests: Query<(
        Entity,
        Option<&ReadRequest>,
        Option<&WriteRequest>,
        Has<Reading>,
        Has<Writing>,
        Has<ToolStore>,
    )>,
    mut commands: Commands,
) {
    for (entity, read, write, reading, writing, source) in &requests {
        if read.is_none() && write.is_none() {
            continue;
        }
        if !source && !reading && !writing {
            if read.is_some() {
                commands
                    .entity(entity)
                    .remove::<ReadRequest>()
                    .insert(ReadResult(Err("tool store is unavailable".into())));
            }
            if write.is_some() {
                commands
                    .entity(entity)
                    .remove::<WriteRequest>()
                    .insert(WriteResult(Err("tool store is unavailable".into())));
            }
        } else if read.is_some() && write.is_some() {
            if !reading {
                commands
                    .entity(entity)
                    .remove::<ReadRequest>()
                    .insert(ReadResult(Err(
                        "another storage request is already pending".into(),
                    )));
            }
            if !writing {
                commands
                    .entity(entity)
                    .remove::<WriteRequest>()
                    .insert(WriteResult(Err(
                        "another storage request is already pending".into(),
                    )));
            }
        }
    }
}

fn poll<T: Component>(
    mut tasks: Query<(Entity, &mut IoTask<T>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let waker = wake
        .as_deref()
        .map(|wake| &wake.0)
        .unwrap_or(std::task::Waker::noop());
    let mut context = Context::from_waker(waker);
    for (entity, mut task) in &mut tasks {
        let Poll::Ready(result) = Pin::new(&mut task.0).poll(&mut context) else {
            continue;
        };
        waker.wake_by_ref();
        commands.entity(entity).remove::<(IoTask<T>, Busy)>();
        match result {
            Ok(result) => {
                commands.entity(entity).insert(result);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failed::<T>(error, std::marker::PhantomData));
            }
        }
    }
}

fn wake(
    pending: Query<
        Entity,
        (
            Or<(
                With<ReadRequest>,
                With<WriteRequest>,
                With<migration::MigrationRequest>,
                With<FileReadRequest>,
                Added<FileReadResult>,
                Added<ReadResult>,
                Added<WriteResult>,
            )>,
            Without<Busy>,
        ),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    if !pending.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::{IoTaskPool, TaskPoolBuilder};
    use std::time::{Duration, Instant};

    struct Fixture {
        app: App,
        store: ToolStore,
        directory: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(directory.path(), directory.path());
            let mut app = App::new();
            app.add_plugins(StoragePlugin);
            Self {
                app,
                store,
                directory,
            }
        }

        fn read(&mut self, retain_lease: bool) -> Entity {
            let entity = self
                .app
                .world_mut()
                .spawn((self.store.clone(), ReadRequest { retain_lease }))
                .id();
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.app.world().get::<ReadResult>(entity).is_none() {
                self.app.update();
                assert!(Instant::now() < deadline, "storage read did not drain");
                std::thread::sleep(Duration::from_millis(2));
            }
            entity
        }

        fn write(&mut self, entity: Entity, manifest: ToolsManifest) {
            self.app
                .world_mut()
                .entity_mut(entity)
                .insert(WriteRequest {
                    manifest,
                    retain_lease: true,
                });
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.app.world().get::<WriteResult>(entity).is_none() {
                self.app.update();
                assert!(Instant::now() < deadline, "storage write did not drain");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    #[test]
    fn canceled_pending_write_preserves_the_committed_manifest() {
        let mut fixture = Fixture::new();
        let mut committed = ToolsManifest::default();
        committed.set_package("npm", "keep", true);
        committed.write_to(&fixture.store.manifest_path()).unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                fixture.store.clone(),
                vmux_ecs::OperationState::Canceled,
                WriteRequest {
                    manifest: ToolsManifest::default(),
                    retain_lease: false,
                },
            ))
            .id();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<WriteResult>(entity)
                .unwrap()
                .0
                .is_err()
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn canceled_storage_work_drains_a_pending_lease_without_writing() {
        for write in [false, true] {
            let mut fixture = Fixture::new();
            let mut committed = ToolsManifest::default();
            committed.set_package("npm", "keep", true);
            committed.write_to(&fixture.store.manifest_path()).unwrap();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            std::fs::write(fixture.store.brewfile_path(), "tap 'preserved'\n").unwrap();
            let blocker = fixture.store.lease().unwrap();
            let entity = fixture
                .app
                .world_mut()
                .spawn((fixture.store.clone(), vmux_ecs::OperationState::Running))
                .id();
            if write {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(entity)
                    .insert(WriteRequest {
                        manifest: ToolsManifest::default(),
                        retain_lease: false,
                    });
            } else {
                fixture
                    .app
                    .world_mut()
                    .entity_mut(entity)
                    .insert(ReadRequest {
                        retain_lease: false,
                    });
            }
            fixture.app.update();
            assert!(fixture.app.world().get::<Busy>(entity).is_some());
            fixture
                .app
                .world_mut()
                .entity_mut(entity)
                .insert(vmux_ecs::OperationState::Canceled);
            fixture.app.update();
            assert!(fixture.app.world().get::<Busy>(entity).is_some());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::OperationRetained>(entity)
                    .is_some()
            );
            drop(blocker);
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                fixture.app.update();
                let completed = if write {
                    fixture
                        .app
                        .world()
                        .get::<WriteResult>(entity)
                        .map(|result| result.0.is_err())
                } else {
                    fixture
                        .app
                        .world()
                        .get::<ReadResult>(entity)
                        .map(|result| result.0.is_err())
                };
                if let Some(failed) = completed {
                    assert!(failed);
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "canceled storage work did not drain"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert_eq!(
                std::fs::read_to_string(fixture.store.brewfile_path()).unwrap(),
                "tap 'preserved'\n"
            );
            assert!(fixture.app.world().get::<Busy>(entity).is_none());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::OperationRetained>(entity)
                    .is_none()
            );
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn legacy_layout_moves_files_before_reconciling_brewfile() {
        let mut fixture = Fixture::new();
        let legacy = fixture.directory.path().join("registry");
        std::fs::create_dir_all(legacy.join("dotfiles/shell")).unwrap();
        std::fs::write(
            legacy.join("registry.toml"),
            "version = 1\n[packages]\nnpm = [\"typescript\"]\n",
        )
        .unwrap();
        std::fs::write(
            legacy.join("Brewfile"),
            "brew \"ripgrep\"\ncask \"ghostty\"\n",
        )
        .unwrap();
        std::fs::write(legacy.join("dotfiles/shell/.zshrc"), "export VMUX=1\n").unwrap();
        let entity = fixture.read(false);
        let manifest = fixture
            .app
            .world()
            .get::<ReadResult>(entity)
            .unwrap()
            .0
            .as_ref()
            .unwrap();
        assert_eq!(manifest.packages["npm"], ["typescript"]);
        assert_eq!(manifest.packages["homebrew-formula"], ["ripgrep"]);
        assert_eq!(manifest.packages["homebrew-cask"], ["ghostty"]);
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read_to_string(fixture.store.root().join("dotfiles/shell/.zshrc")).unwrap(),
            "export VMUX=1\n"
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn requests_without_a_store_fail_instead_of_waiting_forever() {
        let mut fixture = Fixture::new();
        let read = fixture
            .app
            .world_mut()
            .spawn(ReadRequest {
                retain_lease: false,
            })
            .id();
        let write = fixture
            .app
            .world_mut()
            .spawn(WriteRequest {
                manifest: ToolsManifest::default(),
                retain_lease: false,
            })
            .id();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<ReadResult>(read)
                .unwrap()
                .0
                .is_err()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<WriteResult>(write)
                .unwrap()
                .0
                .is_err()
        );
        assert!(!fixture.store.root().exists());
    }

    #[test]
    fn conflicting_requests_cannot_overwrite_one_anothers_io() {
        let mut fixture = Fixture::new();
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                fixture.store.clone(),
                ReadRequest {
                    retain_lease: false,
                },
                WriteRequest {
                    manifest: ToolsManifest::default(),
                    retain_lease: false,
                },
            ))
            .id();
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<ReadResult>(entity)
                .unwrap()
                .0
                .is_err()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<WriteResult>(entity)
                .unwrap()
                .0
                .is_err()
        );
        assert!(fixture.store.lease().is_some());
        assert!(!fixture.store.root().exists());
    }

    #[test]
    fn read_preserves_caller_retention_until_its_own_work_finishes() {
        let mut fixture = Fixture::new();
        let blocker = fixture.store.lease().unwrap();
        let entity = fixture
            .app
            .world_mut()
            .spawn((
                fixture.store.clone(),
                ReadRequest {
                    retain_lease: false,
                },
                vmux_ecs::OperationRetained,
            ))
            .id();
        for _ in 0..3 {
            fixture.app.update();
        }
        assert!(fixture.app.world().get::<ReadResult>(entity).is_none());
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::OperationRetained>(entity)
                .is_some()
        );
        drop(blocker);
        let deadline = Instant::now() + Duration::from_secs(3);
        while fixture.app.world().get::<ReadResult>(entity).is_none() {
            fixture.app.update();
            assert!(Instant::now() < deadline, "blocked read did not drain");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            fixture
                .app
                .world()
                .get::<ReadResult>(entity)
                .unwrap()
                .0
                .is_ok()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_ecs::OperationRetained>(entity)
                .is_some()
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn ambiguous_legacy_layout_refuses_without_overwriting_either_directory() {
        let mut fixture = Fixture::new();
        let legacy = fixture.directory.path().join("registry");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(fixture.store.root()).unwrap();
        std::fs::write(legacy.join("registry.toml"), "legacy").unwrap();
        std::fs::write(fixture.store.manifest_path(), "current").unwrap();
        let entity = fixture.read(false);
        assert!(
            fixture
                .app
                .world()
                .get::<ReadResult>(entity)
                .unwrap()
                .0
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(legacy.join("registry.toml")).unwrap(),
            "legacy"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.store.manifest_path()).unwrap(),
            "current"
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn retained_lease_serializes_read_write_and_preserves_foreign_brewfile_lines() {
        let mut fixture = Fixture::new();
        std::fs::create_dir_all(fixture.store.root()).unwrap();
        let path = fixture.store.brewfile_path();
        std::fs::write(&path, "tap \"homebrew/cask-fonts\"\n# keep this\nbrew \"fd\", link: false\nbrew \"old\"\nmas \"Xcode\", id: 497799835\n").unwrap();
        let entity = fixture.read(true);
        assert!(fixture.store.lease().is_none());
        let mut manifest = fixture
            .app
            .world()
            .get::<ReadResult>(entity)
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .clone();
        manifest.packages.insert(
            "homebrew-formula".into(),
            vec!["fd".into(), "ripgrep".into()],
        );
        manifest
            .packages
            .insert("homebrew-cask".into(), vec!["ghostty".into()]);
        fixture.write(entity, manifest);
        assert!(
            fixture
                .app
                .world()
                .get::<WriteResult>(entity)
                .unwrap()
                .0
                .is_ok()
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# keep this\nbrew \"fd\", link: false\nmas \"Xcode\", id: 497799835\nbrew \"ripgrep\"\ncask \"ghostty\"\n"
        );
        assert!(fixture.store.lease().is_none());
        fixture
            .app
            .world_mut()
            .entity_mut(entity)
            .remove::<StoreLease>();
        assert!(fixture.store.lease().is_some());
        let fresh = fixture.read(false);
        let persisted = fixture
            .app
            .world()
            .get::<ReadResult>(fresh)
            .unwrap()
            .0
            .as_ref()
            .unwrap();
        assert_eq!(persisted.packages["homebrew-formula"], ["fd", "ripgrep"]);
        assert_eq!(persisted.packages["homebrew-cask"], ["ghostty"]);
        assert_eq!(persisted.packages.len(), 2);
    }
}
