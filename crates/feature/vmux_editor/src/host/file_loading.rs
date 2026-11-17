use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::event::FileEncoding;
use vmux_ecs::{
    JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation, OperationState, OperationTarget,
};

use super::edit::{EditCore, highlight_cache::HighlightCache};
use super::editor::{Editor, FileView};
use super::explorer::OutlineDirty;
use super::file_lifecycle::{
    EditorFileLoadedSet, FileBuffer, FileChangesSet, FileDir, ForcedEncoding, LoadFailure,
    MissingFileView,
};
use super::file_loading_io::{Bytes, Directory, Inspect};
use super::highlight::{FILE_VIEW_MAX_BYTES, HIGHLIGHT_MAX_BYTES};
use super::keymap::KeymapConfig;
use super::markdown::ParsedNote;
use super::media::FileMedia;
use super::note::NoteSent;
use super::parked_edits::Parked;

pub(super) struct FileLoadingPlugin;

impl Plugin for FileLoadingPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.add_plugins((
            JobPlugin::<Inspect>::default(),
            JobPlugin::<Directory>::default(),
            JobPlugin::<Bytes>::default(),
        ))
        .add_message::<Failed>()
        .add_systems(
            Update,
            (cancel, accept)
                .chain()
                .after(FileChangesSet)
                .before(JobSet::Submit),
        )
        .add_systems(
            Update,
            (inspected, classify, prepare, listed, read, loaded, failed)
                .chain()
                .in_set(JobSet::Apply)
                .in_set(EditorFileLoadedSet),
        )
        .add_systems(PostUpdate, cancel.before(JobSet::Cleanup));
    }
}

#[derive(Component)]
pub(super) struct FileLoadTask {
    operation: Entity,
}

#[cfg(test)]
impl FileLoadTask {
    pub(super) fn settle(app: &mut App, entity: Entity) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            app.update();
            let world = app.world();
            let loaded = world.get::<FileLoadTask>(entity).is_none()
                && (world.get::<FileBuffer>(entity).is_some()
                    || world.get::<FileDir>(entity).is_some()
                    || world.get::<FileMedia>(entity).is_some()
                    || world.get::<Editor>(entity).is_some());
            if loaded {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "file load did not settle"
            );
            std::thread::yield_now();
        }
    }
}

#[derive(Component)]
struct Load;

#[derive(Component)]
struct Source(PathBuf);

#[derive(Component)]
struct Encoding(FileEncoding);

#[derive(Component)]
struct Heavy;

#[derive(Component)]
struct Metadata(Result<std::fs::Metadata, String>);

#[derive(Component)]
struct Missing;

#[derive(Component)]
struct ReadText;

#[derive(Component)]
struct Text(crate::encoding::DecodedText);

#[derive(Message)]
struct Failed {
    operation: Entity,
    reason: LoadFailure,
    message: String,
    missing: bool,
}

#[derive(SystemParam)]
struct Loads<'w, 's> {
    operations: Query<
        'w,
        's,
        (
            &'static Source,
            &'static OperationTarget,
            Option<&'static Encoding>,
            &'static OperationState,
        ),
        With<Load>,
    >,
    views: Query<'w, 's, (&'static FileView, &'static FileLoadTask)>,
}

impl Loads<'_, '_> {
    fn current(&self, operation: Entity) -> Option<(Entity, &Path, Option<FileEncoding>)> {
        let (source, target, encoding, state) = self.operations.get(operation).ok()?;
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            return None;
        }
        let (view, pending) = self.views.get(target.entity()).ok()?;
        if pending.operation != operation || view.path != source.0 {
            return None;
        }
        Some((
            target.entity(),
            &source.0,
            encoding.map(|encoding| encoding.0),
        ))
    }
}

#[allow(clippy::type_complexity)]
fn accept(
    world: &mut World,
    query: &mut QueryState<
        (Entity, &FileView, Option<&ForcedEncoding>),
        (
            Without<FileBuffer>,
            Without<FileDir>,
            Without<FileMedia>,
            Without<Editor>,
            Without<FileLoadTask>,
        ),
    >,
) {
    let keymap = KeymapConfig::resolve(world.get_resource::<vmux_setting::AppSettings>());
    let mut files = Vec::new();
    for (entity, file, forced) in query.iter(world) {
        let encoding = forced.and_then(|encoding| encoding.for_path(&file.path));
        files.push((entity, file.path.clone(), encoding));
    }
    for (entity, path, encoding) in files {
        if encoding.is_none()
            && let Some((edit, diff)) = Parked::resume(world, entity, &path)
        {
            world
                .entity_mut(entity)
                .insert((edit, keymap.keymap(), diff))
                .remove::<MissingFileView>();
            if ParsedNote::supports(&path) {
                world
                    .entity_mut(entity)
                    .remove::<NoteSent>()
                    .insert(OutlineDirty);
            }
            continue;
        }
        let operation = world
            .spawn((
                Operation::bundle("editor.file.load").unwrap().running(),
                Name::new("Load editor file"),
                Load,
                Source(path.clone()),
                OperationTarget::new(entity),
            ))
            .id();
        if let Some(encoding) = encoding {
            world.entity_mut(operation).insert(Encoding(encoding));
        }
        world.spawn((
            Operation::bundle("editor.file.inspect").unwrap(),
            Name::new("Inspect editor file"),
            ChildOf(operation),
            JobRequest(Inspect { path: path.clone() }),
        ));
        world.entity_mut(entity).insert(FileLoadTask { operation });
    }
}

fn cancel(
    operations: Query<(Entity, &OperationState, Option<&OperationTarget>), With<Load>>,
    loads: Loads,
    mut commands: Commands,
) {
    for (operation, state, target) in &operations {
        if loads.current(operation).is_some() {
            continue;
        }
        if let Some(target) = target
            && let Ok((view, pending)) = loads.views.get(target.entity())
            && pending.operation == operation
        {
            if !matches!(state, OperationState::Pending | OperationState::Running)
                && loads
                    .operations
                    .get(operation)
                    .is_ok_and(|(source, _, _, _)| source.0 == view.path)
            {
                let message = match state {
                    OperationState::Canceled => "file loading canceled",
                    _ => "file loading stopped before content publication",
                };
                commands
                    .entity(target.entity())
                    .insert(FileBuffer::failed(LoadFailure::Fatal, message.into()))
                    .remove::<MissingFileView>();
            }
            commands.entity(target.entity()).remove::<FileLoadTask>();
        }
        commands.entity(operation).despawn();
    }
}

fn inspected(
    jobs: Query<(Entity, &JobResult<Inspect>, &ChildOf)>,
    loads: Loads,
    mut commands: Commands,
) {
    for (job, result, parent) in &jobs {
        let operation = parent.parent();
        commands.entity(job).despawn();
        if loads.current(operation).is_none() {
            continue;
        }
        let metadata = match &result.0 {
            Ok(result) => result.as_ref().cloned().map_err(|error| error.to_string()),
            Err(error) => Err(error.clone()),
        };
        commands.entity(operation).insert(Metadata(metadata));
        if matches!(&result.0, Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound) {
            commands.entity(operation).insert(Missing);
        }
    }
}

fn classify(
    inspected: Query<(Entity, &Metadata), Without<ReadText>>,
    loads: Loads,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    for (operation, metadata) in &inspected {
        let Some((view, path, _)) = loads.current(operation) else {
            continue;
        };
        if metadata.0.as_ref().is_ok_and(|metadata| metadata.is_dir()) {
            commands.spawn((
                Operation::bundle("editor.file.directory").unwrap(),
                Name::new("Read editor directory"),
                ChildOf(operation),
                JobRequest(Directory {
                    path: path.to_path_buf(),
                }),
            ));
            commands.entity(operation).remove::<Metadata>();
            if let Some(wake) = &wake {
                let _ = catch_unwind(AssertUnwindSafe(|| wake.0.wake_by_ref()));
            }
        } else if let Some(kind) = vmux_api::media::MediaKind::from_path(&path.to_string_lossy()) {
            let mime = vmux_api::media::MediaKind::mime(&path.to_string_lossy())
                .unwrap_or("application/octet-stream")
                .to_string();
            commands
                .entity(view)
                .insert(FileMedia { kind, mime })
                .remove::<(FileLoadTask, MissingFileView)>();
            commands.entity(operation).despawn();
            continue;
        } else {
            commands.entity(operation).insert(ReadText);
        }
    }
}

fn prepare(
    inspected: Query<(Entity, &Metadata, Has<Missing>), With<ReadText>>,
    loads: Loads,
    wake: Option<Res<JobWake>>,
    mut failures: MessageWriter<Failed>,
    mut commands: Commands,
) {
    for (operation, metadata, missing) in &inspected {
        let Some((_, path, _)) = loads.current(operation) else {
            continue;
        };
        commands.entity(operation).remove::<(Metadata, ReadText)>();
        let size = match &metadata.0 {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                failures.write(Failed {
                    operation,
                    reason: LoadFailure::Fatal,
                    message: format!("cannot open {}: {error}", path.display()),
                    missing,
                });
                continue;
            }
        };
        if size > FILE_VIEW_MAX_BYTES {
            failures.write(Failed {
                operation,
                reason: LoadFailure::Fatal,
                message: format!("file too large ({size} bytes, max {})", FILE_VIEW_MAX_BYTES),
                missing: false,
            });
            continue;
        }
        if size > HIGHLIGHT_MAX_BYTES {
            commands.entity(operation).insert(Heavy);
        }
        commands.spawn((
            Operation::bundle("editor.file.bytes").unwrap(),
            Name::new("Read editor file bytes"),
            ChildOf(operation),
            JobRequest(Bytes {
                path: path.to_path_buf(),
                maximum: FILE_VIEW_MAX_BYTES,
            }),
        ));
        if let Some(wake) = &wake {
            let _ = catch_unwind(AssertUnwindSafe(|| wake.0.wake_by_ref()));
        }
    }
}

fn listed(
    jobs: Query<(Entity, &JobResult<Directory>, &ChildOf)>,
    loads: Loads,
    mut failures: MessageWriter<Failed>,
    mut commands: Commands,
) {
    for (job, result, parent) in &jobs {
        let operation = parent.parent();
        commands.entity(job).despawn();
        let Some((view, _, _)) = loads.current(operation) else {
            continue;
        };
        let mut entries = match &result.0 {
            Ok(Ok(entries)) => entries.clone(),
            Ok(Err(_)) => Vec::new(),
            Err(error) => {
                failures.write(Failed {
                    operation,
                    reason: LoadFailure::Fatal,
                    message: error.clone(),
                    missing: false,
                });
                continue;
            }
        };
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        commands
            .entity(view)
            .insert(FileDir { entries })
            .remove::<(FileLoadTask, MissingFileView)>();
        commands.entity(operation).despawn();
    }
}

fn read(
    jobs: Query<(Entity, &JobResult<Bytes>, &ChildOf)>,
    loads: Loads,
    mut failures: MessageWriter<Failed>,
    mut commands: Commands,
) {
    for (job, result, parent) in &jobs {
        let operation = parent.parent();
        commands.entity(job).despawn();
        let Some((_, path, encoding)) = loads.current(operation) else {
            continue;
        };
        let bytes = match &result.0 {
            Ok(result) => result.as_ref().map_err(|error| error.to_string()),
            Err(error) => Err(error.clone()),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                let missing = matches!(&result.0, Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound);
                failures.write(Failed {
                    operation,
                    reason: LoadFailure::Fatal,
                    message: format!("cannot read {}: {error}", path.display()),
                    missing,
                });
                continue;
            }
        };
        let decoded = match encoding {
            Some(encoding) => crate::encoding::DecodedText::forced(bytes, encoding),
            None => match crate::encoding::DecodedText::decode(bytes) {
                Some(decoded) => decoded,
                None => {
                    failures.write(Failed {
                        operation,
                        reason: LoadFailure::Undecodable,
                        message: format!("not a text file: {}", path.display()),
                        missing: false,
                    });
                    continue;
                }
            },
        };
        commands.entity(operation).insert(Text(decoded));
    }
}

fn loaded(
    texts: Query<(Entity, &Text, Has<Heavy>)>,
    loads: Loads,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    let keymap = KeymapConfig::resolve(settings.as_deref());
    for (operation, text, heavy) in &texts {
        let Some((view, path, _)) = loads.current(operation) else {
            continue;
        };
        let highlight = match heavy {
            true => HighlightCache::plain(path),
            false => HighlightCache::new(path),
        };
        let mut core = EditCore::new(
            path.to_path_buf(),
            highlight.language.clone(),
            &text.0.text,
            keymap.initial_mode(),
        );
        core.buffer.encoding = text.0.encoding;
        let mut folds = crate::fold::FoldState::default();
        if !heavy {
            folds.set_indented(&core.buffer.rope);
        }
        core.fold_view = folds.view(core.buffer.len_lines() as u32);
        commands
            .entity(view)
            .insert((
                Editor::new(core, highlight, folds),
                keymap.keymap(),
                vmux_git::GitDiffSource {
                    content: text.0.text.clone(),
                    dirty: false,
                },
            ))
            .remove::<(FileLoadTask, MissingFileView)>();
        if ParsedNote::supports(path) {
            commands
                .entity(view)
                .remove::<NoteSent>()
                .insert(OutlineDirty);
        }
        commands.entity(operation).despawn();
    }
}

fn failed(mut failures: MessageReader<Failed>, loads: Loads, mut commands: Commands) {
    for failure in failures.read() {
        let Some((view, _, _)) = loads.current(failure.operation) else {
            continue;
        };
        commands
            .entity(view)
            .insert(FileBuffer::failed(failure.reason, failure.message.clone()))
            .remove::<FileLoadTask>();
        if failure.missing {
            commands.entity(view).insert(MissingFileView);
        } else {
            commands.entity(view).remove::<MissingFileView>();
        }
        commands.entity(failure.operation).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::Duration;

    use vmux_ecs::blocking::{BlockingPool, BlockingRuntime, BlockingTask, Lane};
    use vmux_ecs::profile::ActiveProfile;

    use super::super::fold_io::{FileStore, FileWake};
    use super::super::fold_preferences::{Root, Snapshot};

    use super::*;

    struct HostWake(mpsc::Sender<()>);

    impl Wake for HostWake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    struct BlockedWorker {
        task: BlockingTask<()>,
        release: Option<mpsc::SyncSender<()>>,
    }

    impl BlockedWorker {
        fn new(runtime: &BlockingRuntime) -> Self {
            let (started, running) = mpsc::sync_channel(1);
            let (release, blocked) = mpsc::sync_channel(1);
            let task = runtime
                .try_spawn(Lane::Short, None, move || {
                    started.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                })
                .unwrap();
            running.recv_timeout(Duration::from_secs(5)).unwrap();
            Self {
                task,
                release: Some(release),
            }
        }

        fn release(mut self) {
            self.release.take().unwrap().send(()).unwrap();
        }
    }

    impl Drop for BlockedWorker {
        fn drop(&mut self) {
            if let Some(release) = self.release.take() {
                let _ = release.send(());
            }
            self.task.cancel();
        }
    }

    struct Host {
        app: App,
        view: Entity,
        directory: tempfile::TempDir,
        wake: mpsc::Receiver<()>,
    }

    impl Host {
        fn new(name: &str, content: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join(name);
            std::fs::write(&path, content).unwrap();
            let (sender, wake) = mpsc::channel();
            let mut app = App::new();
            app.add_plugins(super::super::file_lifecycle::FileLifecyclePlugin)
                .insert_non_send(bevy_cef::prelude::Browsers::default())
                .insert_resource(JobWake(Arc::new(HostWake(sender)).into()))
                .insert_resource(BlockingRuntime::new(
                    BlockingPool::new("file-loading", 1, 1),
                    BlockingPool::new("file-loading-long", 0, 0),
                    BlockingPool::new("file-loading-process", 0, 0),
                ));
            let view = app.world_mut().spawn(FileView { path }).id();
            Self {
                app,
                view,
                directory,
                wake,
            }
        }

        fn frame(&mut self) {
            self.wake.recv_timeout(Duration::from_secs(5)).unwrap();
            self.app.update();
        }

        fn ready(&mut self) {
            while self.app.world().get::<FileLoadTask>(self.view).is_some() {
                self.frame();
            }
        }

        fn inspected(&mut self) {
            let worker = BlockedWorker::new(self.app.world().resource::<BlockingRuntime>());
            self.app.update();
            worker.release();
            self.frame();
        }
    }

    #[test]
    fn loaded_editor_projects_fold_preferences_from_the_same_native_runtime() {
        let mut host = Host::new("file.rs", "fn main() {\n    work();\n}\n");
        let path =
            vmux_path::PathIdentity::resolve(host.directory.path().join("file.rs")).into_path_buf();
        let store = host.directory.path().join("folds.ron");
        FileStore::write(
            &store,
            &Snapshot {
                files: [(path.to_string_lossy().into_owned(), vec![0])].into(),
            },
        )
        .unwrap();
        let wake = host.app.world().resource::<JobWake>().0.clone();
        host.app
            .add_plugins(super::super::fold_store::FoldStorePlugin)
            .insert_resource(FileWake(wake));
        host.app.world_mut().spawn((
            Root(store),
            ActiveProfile(vmux_ecs::profile::Profile::current()),
        ));
        host.app.update();
        host.ready();
        loop {
            let editor = host.app.world().get::<Editor>(host.view).unwrap();
            if editor.folds.collapsed.contains(&0) {
                break;
            }
            host.frame();
        }
        assert_eq!(
            host.app
                .world()
                .get::<Editor>(host.view)
                .unwrap()
                .core
                .buffer
                .text(),
            "fn main() {\n    work();\n}\n"
        );
    }

    #[test]
    fn real_file_publication_finishes_using_only_native_woken_frames() {
        let mut host = Host::new("file.rs", "fn main() {}\n");
        host.app.update();
        host.ready();
        assert_eq!(
            host.app
                .world()
                .get::<Editor>(host.view)
                .unwrap()
                .core
                .buffer
                .text(),
            "fn main() {}\n"
        );
    }

    #[test]
    fn path_replacement_refuses_a_submitted_read_for_the_previous_file() {
        let mut host = Host::new("previous.rs", "previous\n");
        let latest = host.directory.path().join("latest.rs");
        std::fs::write(&latest, "latest\n").unwrap();
        host.inspected();
        let worker = BlockedWorker::new(host.app.world().resource::<BlockingRuntime>());
        host.app.update();
        host.app
            .world_mut()
            .get_mut::<FileView>(host.view)
            .unwrap()
            .path = latest;
        host.app.update();
        worker.release();
        host.ready();
        assert_eq!(
            host.app
                .world()
                .get::<Editor>(host.view)
                .unwrap()
                .core
                .buffer
                .text(),
            "latest\n"
        );
    }

    #[test]
    fn canceling_the_accepted_operation_stops_its_page_load_without_retry() {
        let mut host = Host::new("file.rs", "canceled\n");
        host.inspected();
        let worker = BlockedWorker::new(host.app.world().resource::<BlockingRuntime>());
        host.app.update();
        let operation = host
            .app
            .world()
            .get::<FileLoadTask>(host.view)
            .unwrap()
            .operation;
        host.app
            .world_mut()
            .entity_mut(operation)
            .insert(OperationState::Canceled);
        host.app.update();
        worker.release();
        host.ready();
        host.frame();
        assert!(host.app.world().get::<Editor>(host.view).is_none());
        assert!(
            host.app
                .world()
                .get::<FileBuffer>(host.view)
                .unwrap()
                .load_error()
                .is_some()
        );
    }

    #[test]
    fn view_removal_keeps_native_drain_owned_until_the_submitted_read_retires() {
        let mut host = Host::new("file.rs", "retired\n");
        host.inspected();
        let worker = BlockedWorker::new(host.app.world().resource::<BlockingRuntime>());
        host.app.update();
        host.app.world_mut().despawn(host.view);
        host.app.update();
        let retained = host
            .app
            .world_mut()
            .query_filtered::<Entity, With<Operation>>()
            .iter(host.app.world())
            .count();
        assert!(retained > 0);
        worker.release();
        host.frame();
        let remaining = host
            .app
            .world_mut()
            .query_filtered::<Entity, With<Operation>>()
            .iter(host.app.world())
            .count();
        assert_eq!(remaining, 0);
        assert!(host.app.world().get_entity(host.view).is_err());
    }

    #[test]
    fn directory_loading_applies_native_entries_in_directories_first_name_order() {
        let mut host = Host::new("b.txt", "");
        std::fs::write(host.directory.path().join("a.txt"), "").unwrap();
        std::fs::create_dir(host.directory.path().join("zdir")).unwrap();
        host.app
            .world_mut()
            .get_mut::<FileView>(host.view)
            .unwrap()
            .path = host.directory.path().to_path_buf();
        host.app.update();
        host.ready();
        let names: Vec<_> = host
            .app
            .world()
            .get::<FileDir>(host.view)
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["zdir", "a.txt", "b.txt"]);
    }

    #[test]
    fn file_growth_is_refused_by_the_accepted_native_read_limit() {
        let mut host = Host::new("growing.txt", "123");
        let path = host.directory.path().join("growing.txt");
        let job = host
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.file.read").unwrap(),
                JobRequest(Bytes {
                    path: path.clone(),
                    maximum: 4,
                }),
            ))
            .id();
        std::fs::write(&path, "123456789").unwrap();
        host.app.update();
        while host.app.world().get::<JobResult<Bytes>>(job).is_none() {
            host.frame();
        }
        assert!(
            host.app
                .world()
                .get::<JobResult<Bytes>>(job)
                .unwrap()
                .0
                .as_ref()
                .unwrap()
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "123456789");
    }
}
