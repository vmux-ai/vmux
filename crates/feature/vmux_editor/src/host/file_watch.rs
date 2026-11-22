use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_ecs::{Active, Cwd, JobWake};

use super::editor::FileView;
use super::explorer::ExplorerTree;
use super::file_lifecycle::FileChangesSet;
use super::file_watch_io::Transport;

pub(super) struct FileWatchPlugin;

impl Plugin for FileWatchPlugin {
    fn build(&self, app: &mut App) {
        match Transport::new() {
            Ok(transport) => {
                app.insert_non_send(transport);
            }
            Err(error) => tracing::warn!("file watcher init failed: {error}"),
        }
        app.add_message::<Observed>()
            .add_message::<Reconciled>()
            .add_systems(
                Update,
                (wake, receive, invalidate, select, retire, register)
                    .chain()
                    .before(FileChangesSet),
            );
    }
}

#[derive(Message)]
pub(super) struct Observed(pub notify::Result<notify::Event>);

#[derive(Message)]
pub(super) struct Reconciled(pub PathBuf);

#[derive(Component)]
struct Watch;

#[derive(Component)]
struct Watching;

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct Reset;

fn wake(transport: Option<NonSend<Transport>>, wake: Option<Res<JobWake>>) {
    if let Some(transport) = transport
        && let Some(wake) = wake
    {
        transport.bind_wake(wake.0.clone());
    }
}

fn select(
    views: Query<&FileView>,
    trees: Query<&ExplorerTree>,
    registrations: Query<(Entity, &Cwd, Has<Active>), With<Watch>>,
    mut commands: Commands,
) {
    let mut wanted = HashSet::new();
    for view in &views {
        if let Some(directory) = nearest_directory(&view.path) {
            wanted.insert(vmux_path::PathIdentity::resolve(directory).into_path_buf());
        }
    }
    for tree in &trees {
        for directory in tree.expanded_dirs() {
            wanted.insert(vmux_path::PathIdentity::resolve(directory).into_path_buf());
        }
    }
    let mut existing = HashSet::new();
    for (entity, path, active) in &registrations {
        existing.insert(path.0.clone());
        if wanted.contains(&path.0) {
            if !active {
                commands.entity(entity).insert(Active);
            }
        } else if active {
            commands.entity(entity).remove::<Active>();
        }
    }
    for directory in wanted {
        if !existing.contains(&directory) {
            commands.spawn((
                Name::new("Editor directory watch"),
                Cwd(directory),
                Active,
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

type PendingRegistrations<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Cwd, Option<&'static Failure>),
    (With<Watch>, With<Active>, Without<Watching>),
>;

fn register(
    registrations: PendingRegistrations,
    transport: Option<NonSendMut<Transport>>,
    mut reconciled: MessageWriter<Reconciled>,
    mut commands: Commands,
) {
    let Some(mut transport) = transport else {
        return;
    };
    for (entity, path, failure) in &registrations {
        match transport.watch(&path.0) {
            Ok(()) => {
                reconciled.write(Reconciled(path.0.clone()));
                commands.entity(entity).insert(Watching).remove::<Failure>();
            }
            Err(error) => {
                let message = error.to_string();
                if failure.is_none_or(|failure| failure.0 != message) {
                    tracing::warn!(path = %path.0.display(), "file watch failed: {message}");
                    commands.entity(entity).insert(Failure(message));
                }
            }
        }
    }
}

type RetiringRegistrations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Cwd,
        Has<Watching>,
        Has<Active>,
        Option<&'static Failure>,
    ),
    (With<Watch>, Or<(Without<Active>, With<Reset>)>),
>;

fn retire(
    registrations: RetiringRegistrations,
    transport: Option<NonSendMut<Transport>>,
    mut commands: Commands,
) {
    let Some(mut transport) = transport else {
        return;
    };
    for (entity, path, watching, active, failure) in &registrations {
        if watching {
            match transport.unwatch(&path.0) {
                Ok(()) => {}
                Err(error) if matches!(error.kind, notify::ErrorKind::WatchNotFound) => {}
                Err(error) => {
                    let message = error.to_string();
                    if failure.is_none_or(|failure| failure.0 != message) {
                        tracing::warn!(path = %path.0.display(), "file unwatch failed: {message}");
                        commands.entity(entity).insert(Failure(message));
                    }
                    continue;
                }
            }
        }
        if active {
            commands
                .entity(entity)
                .remove::<(Watching, Reset, Failure)>();
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn invalidate(
    mut observations: MessageReader<Observed>,
    registrations: Query<(Entity, &Cwd), With<Watching>>,
    mut commands: Commands,
) {
    for observation in observations.read() {
        let Ok(event) = &observation.0 else {
            continue;
        };
        if !matches!(
            event.kind,
            notify::EventKind::Remove(_)
                | notify::EventKind::Modify(notify::event::ModifyKind::Name(
                    notify::event::RenameMode::From
                        | notify::event::RenameMode::Both
                        | notify::event::RenameMode::Any
                ))
        ) {
            continue;
        }
        for (entity, directory) in &registrations {
            if event.paths.contains(&directory.0) {
                commands.entity(entity).insert(Reset);
            }
        }
    }
}

fn receive(transport: Option<NonSend<Transport>>, mut observations: MessageWriter<Observed>) {
    let Some(transport) = transport else {
        return;
    };
    while let Some(observation) = transport.receive() {
        observations.write(Observed(observation));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::{Duration, Instant};

    use bevy_cef::prelude::Browsers;

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
                .query_filtered::<&Cwd, With<Watching>>();
            for path in registrations.iter(self.app.world()) {
                paths.push(path.0.clone());
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
            .resource::<Messages<Reconciled>>()
            .get_cursor();
        cursor
            .read(fixture.app.world().resource::<Messages<Reconciled>>())
            .for_each(drop);
        fixture
            .app
            .world_mut()
            .write_message(Observed(Ok(notify::Event::new(notify::EventKind::Remove(
                notify::event::RemoveKind::Folder,
            ))
            .add_path(directory.clone()))));
        fixture.app.update();
        let roots: Vec<_> = cursor
            .read(fixture.app.world().resource::<Messages<Reconciled>>())
            .map(|fact| fact.0.clone())
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
        fixture
            .app
            .world_mut()
            .write_message(Observed(Ok(notify::Event::new(notify::EventKind::Access(
                notify::event::AccessKind::Read,
            ))
            .add_path(fixture.directory.path().join("file.txt")))));
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
