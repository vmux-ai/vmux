use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_ecs::watch::{FileWatch, FileWatchPlugin, FileWatchSet};
use vmux_path::PathIdentity;

use super::editor::FileView;
use super::explorer::{ExplorerDirectory, ExplorerDirectoryExpanded};
use super::file_lifecycle::FileChangesSet;

pub(super) struct EditorFileWatchPlugin;

impl Plugin for EditorFileWatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FileWatchPlugin)
            .configure_sets(Update, FileWatchSet::Reconcile.before(FileChangesSet))
            .add_systems(
                Update,
                select
                    .after(FileWatchSet::Receive)
                    .before(FileWatchSet::Reconcile),
            );
    }
}

#[derive(Component)]
struct Watch;

fn select(
    views: Query<&FileView>,
    expanded: Query<&ExplorerDirectory, With<ExplorerDirectoryExpanded>>,
    registrations: Query<(Entity, &FileWatch), With<Watch>>,
    mut commands: Commands,
) {
    let mut wanted = HashSet::new();
    for view in &views {
        if let Some(directory) = nearest_directory(&view.path) {
            wanted.insert(PathIdentity::resolve(directory).into_path_buf());
        }
    }
    for directory in &expanded {
        wanted.insert(PathIdentity::resolve(&directory.path).into_path_buf());
    }
    let mut existing = HashSet::new();
    for (entity, watch) in &registrations {
        let path = watch.path().to_path_buf();
        if wanted.contains(&path) {
            existing.insert(path);
        } else {
            commands.entity(entity).despawn();
        }
    }
    for directory in wanted {
        if !existing.contains(&directory) {
            commands.spawn((
                Name::new("Editor directory watch"),
                FileWatch::non_recursive(directory),
                Watch,
            ));
        }
    }
}

fn nearest_directory(path: &Path) -> Option<PathBuf> {
    let mut directory = if path.is_dir() { path } else { path.parent()? };
    loop {
        if directory.is_dir() {
            return Some(directory.to_path_buf());
        }
        directory = directory.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::{Duration, Instant};

    use bevy_cef::prelude::Browsers;
    use vmux_ecs::JobWake;
    use vmux_ecs::watch::{FileWatchEvent, FileWatchEventKind, FileWatchReconciled, Watching};

    use super::super::editor::Editor;
    use super::super::file_lifecycle::{FileLifecyclePlugin, FileLoadTask};
    use super::*;

    struct HostWake(mpsc::Sender<()>);

    impl Wake for HostWake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        wake: mpsc::Receiver<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let (sender, wake) = mpsc::channel();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, FileLifecyclePlugin))
                .insert_non_send(Browsers::default())
                .insert_resource(JobWake(Arc::new(HostWake(sender)).into()));
            Self {
                app,
                directory,
                wake,
            }
        }

        fn open(&mut self, name: &str) -> Entity {
            let path = self.directory.path().join(name);
            std::fs::write(&path, "original\n").unwrap();
            let view = self.app.world_mut().spawn(FileView { path }).id();
            FileLoadTask::settle(&mut self.app, view);
            view
        }

        fn registrations(&mut self) -> Vec<PathBuf> {
            let mut paths = Vec::new();
            let mut registrations = self
                .app
                .world_mut()
                .query_filtered::<&FileWatch, (With<Watch>, With<Watching>)>();
            for watch in registrations.iter(self.app.world()) {
                paths.push(watch.path().to_path_buf());
            }
            paths.sort();
            paths
        }

        fn discard_wakes(&self) {
            while self.wake.try_recv().is_ok() {}
        }
    }

    #[test]
    fn shared_directory_watch_retires_only_after_the_last_view_closes() {
        let mut fixture = Fixture::new();
        let first = fixture.open("first.txt");
        let second = fixture.open("second.txt");
        let directory = fixture.directory.path().canonicalize().unwrap();
        assert_eq!(
            fixture.registrations().as_slice(),
            std::slice::from_ref(&directory)
        );
        fixture.app.world_mut().despawn(first);
        fixture.app.update();
        assert_eq!(fixture.registrations(), [directory]);
        fixture.app.world_mut().despawn(second);
        fixture.app.update();
        assert!(fixture.registrations().is_empty());
    }

    #[test]
    fn removed_directory_observation_reconciles_an_active_recreated_registration() {
        let mut fixture = Fixture::new();
        fixture.open("file.txt");
        let directory = fixture.directory.path().canonicalize().unwrap();
        let mut cursor = fixture
            .app
            .world()
            .resource::<Messages<FileWatchReconciled>>()
            .get_cursor();
        cursor
            .read(
                fixture
                    .app
                    .world()
                    .resource::<Messages<FileWatchReconciled>>(),
            )
            .for_each(drop);
        fixture
            .app
            .world_mut()
            .write_message(FileWatchEvent::directory_removed(vec![directory.clone()]));
        fixture.app.update();
        let roots: Vec<_> = cursor
            .read(
                fixture
                    .app
                    .world()
                    .resource::<Messages<FileWatchReconciled>>(),
            )
            .map(|fact| fact.path.clone())
            .collect();
        assert_eq!(roots.as_slice(), std::slice::from_ref(&directory));
        assert_eq!(fixture.registrations(), [directory]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_deleted_and_recreated_directory_watches_reload_later_edits() {
        let mut fixture = Fixture::new();
        let directory = fixture.directory.path().join("recreated");
        std::fs::create_dir(&directory).unwrap();
        let view = fixture.open("recreated/file.txt");
        fixture.discard_wakes();
        std::fs::remove_dir_all(&directory).unwrap();
        fixture.wake.recv_timeout(Duration::from_secs(5)).unwrap();
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("file.txt");
        std::fs::write(&path, "recreated\n").unwrap();
        fixture.app.update();
        FileLoadTask::settle(&mut fixture.app, view);
        fixture.discard_wakes();
        std::fs::write(&path, "later\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture
                .wake
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            fixture.app.update();
            if Editor::read(fixture.app.world(), view)
                .is_some_and(|editor| editor.core.buffer.text() == "later\n")
            {
                break;
            }
        }
    }

    #[test]
    fn moving_a_view_replaces_its_native_directory_registration() {
        let mut fixture = Fixture::new();
        let view = fixture.open("first.txt");
        let next = fixture.directory.path().join("next");
        std::fs::create_dir(&next).unwrap();
        let path = next.join("next.txt");
        std::fs::write(&path, "next\n").unwrap();
        fixture
            .app
            .world_mut()
            .get_mut::<FileView>(view)
            .unwrap()
            .path = path;
        fixture.app.update();
        assert_eq!(fixture.registrations(), [next.canonicalize().unwrap()]);
    }

    #[test]
    fn native_file_changes_wake_the_registered_reload_pipeline() {
        let mut fixture = Fixture::new();
        let view = fixture.open("file.txt");
        fixture.discard_wakes();
        std::fs::write(fixture.directory.path().join("file.txt"), "changed\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            fixture.wake.recv_timeout(remaining).unwrap();
            fixture.app.update();
            if Editor::read(fixture.app.world(), view)
                .is_some_and(|editor| editor.core.buffer.text() == "changed\n")
            {
                break;
            }
        }
    }

    #[test]
    fn access_observations_do_not_retire_a_loaded_editor() {
        let mut fixture = Fixture::new();
        let view = fixture.open("file.txt");
        let _ = std::fs::read_to_string(fixture.directory.path().join("file.txt")).unwrap();
        fixture
            .app
            .world_mut()
            .write_message(FileWatchEvent::changed(
                Vec::new(),
                FileWatchEventKind::Other,
            ));
        fixture.app.update();
        assert_eq!(
            Editor::read(fixture.app.world(), view)
                .unwrap()
                .core
                .buffer
                .text(),
            "original\n"
        );
    }

    #[test]
    fn missing_file_watch_moves_from_the_existing_ancestor_to_the_created_directory() {
        let mut fixture = Fixture::new();
        let parent = fixture.directory.path().join("created");
        let path = parent.join("file.txt");
        let view = fixture
            .app
            .world_mut()
            .spawn(FileView { path: path.clone() })
            .id();
        FileLoadTask::settle(&mut fixture.app, view);
        assert_eq!(
            fixture.registrations(),
            [fixture.directory.path().canonicalize().unwrap()]
        );
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(&path, "created\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if Editor::read(fixture.app.world(), view).is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(fixture.registrations(), [parent.canonicalize().unwrap()]);
        assert_eq!(
            Editor::read(fixture.app.world(), view)
                .unwrap()
                .core
                .buffer
                .text(),
            "created\n"
        );
    }
}
