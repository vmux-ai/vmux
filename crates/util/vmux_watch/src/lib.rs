use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, PreStartup, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_channel::{AsyncChannelPlugin, MessageSender};
use vmux_path::PathIdentity;

pub struct FileWatchPlugin;

impl Plugin for FileWatchPlugin {
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WatchRuntimePlugin>() {
            app.add_plugins(WatchRuntimePlugin);
        }
    }
}

struct WatchRuntimePlugin;

impl Plugin for WatchRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AsyncChannelPlugin::message::<FileWatchEvent>())
            .add_message::<FileWatchReconciled>()
            .configure_sets(
                Update,
                (FileWatchSet::Receive, FileWatchSet::Reconcile).chain(),
            )
            .add_systems(PreStartup, start)
            .add_systems(
                Update,
                (
                    invalidate.in_set(FileWatchSet::Receive),
                    reconcile.in_set(FileWatchSet::Reconcile),
                ),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileWatchSet {
    Receive,
    Reconcile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WatchMode {
    NonRecursive,
    Recursive,
}

impl WatchMode {
    fn to_notify(self) -> RecursiveMode {
        match self {
            Self::NonRecursive => RecursiveMode::NonRecursive,
            Self::Recursive => RecursiveMode::Recursive,
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[component(on_remove = clear_state)]
pub struct FileWatch {
    path: PathBuf,
    mode: WatchMode,
}

impl FileWatch {
    pub fn recursive(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            mode: WatchMode::Recursive,
        }
    }

    pub fn non_recursive(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            mode: WatchMode::NonRecursive,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mode(&self) -> WatchMode {
        self.mode
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watching;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct WatchError(String);

impl WatchError {
    pub fn message(&self) -> &str {
        &self.0
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatchReset;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileWatchEventKind {
    Created,
    Modified,
    Removed,
    Renamed,
    Other,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct FileWatchEvent {
    pub paths: Vec<PathBuf>,
    pub kind: FileWatchEventKind,
    pub directory_removed: bool,
    pub error: Option<String>,
}

impl FileWatchEvent {
    pub fn changed(paths: Vec<PathBuf>, kind: FileWatchEventKind) -> Self {
        Self {
            paths,
            kind,
            directory_removed: false,
            error: None,
        }
    }

    pub fn directory_removed(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            kind: FileWatchEventKind::Removed,
            directory_removed: true,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            paths: Vec::new(),
            kind: FileWatchEventKind::Other,
            directory_removed: false,
            error: Some(message.into()),
        }
    }

    fn from_notify(result: notify::Result<notify::Event>) -> Option<Self> {
        match result {
            Ok(event) if matches!(event.kind, notify::EventKind::Access(_)) => None,
            Ok(event) => {
                let kind = match event.kind {
                    notify::EventKind::Create(_) => FileWatchEventKind::Created,
                    notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                        FileWatchEventKind::Renamed
                    }
                    notify::EventKind::Modify(_) => FileWatchEventKind::Modified,
                    notify::EventKind::Remove(_) => FileWatchEventKind::Removed,
                    notify::EventKind::Any
                    | notify::EventKind::Other
                    | notify::EventKind::Access(_) => FileWatchEventKind::Other,
                };
                let directory_removed = matches!(
                    event.kind,
                    notify::EventKind::Remove(_)
                        | notify::EventKind::Modify(notify::event::ModifyKind::Name(
                            notify::event::RenameMode::From
                                | notify::event::RenameMode::Both
                                | notify::event::RenameMode::Any
                        ))
                );
                let paths = event
                    .paths
                    .into_iter()
                    .map(|path| PathIdentity::resolve(path).into_path_buf())
                    .collect();
                Some(Self {
                    paths,
                    kind,
                    directory_removed,
                    error: None,
                })
            }
            Err(error) => Some(Self::error(error.to_string())),
        }
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct FileWatchReconciled {
    pub path: PathBuf,
}

struct WatchDriver {
    watcher: RecommendedWatcher,
    registered: HashMap<PathBuf, WatchMode>,
}

impl WatchDriver {
    fn new(sender: MessageSender<FileWatchEvent>) -> notify::Result<Self> {
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if let Some(event) = FileWatchEvent::from_notify(result) {
                let _ = sender.send(event);
            }
        })?;
        Ok(Self {
            watcher,
            registered: HashMap::new(),
        })
    }

    fn is_watching(&self, path: &Path) -> bool {
        self.registered.contains_key(path)
    }

    fn unwatch_stale(&mut self, desired: &HashMap<PathBuf, (WatchMode, bool)>) {
        let stale: Vec<PathBuf> = self
            .registered
            .keys()
            .filter(|path| !desired.contains_key(*path))
            .cloned()
            .collect();
        for path in stale {
            match self.watcher.unwatch(&path) {
                Ok(()) => {}
                Err(error) if matches!(error.kind, notify::ErrorKind::WatchNotFound) => {}
                Err(error) => {
                    tracing::warn!(path = %path.display(), "file unwatch failed: {error}");
                }
            }
            self.registered.remove(&path);
        }
    }

    fn sync_path(&mut self, path: &Path, mode: WatchMode, reset: bool) -> Result<bool, String> {
        if let Some(&current) = self.registered.get(path) {
            if current == mode && !reset {
                return Ok(false);
            }
            match self.watcher.unwatch(path) {
                Ok(()) => {}
                Err(error) if matches!(error.kind, notify::ErrorKind::WatchNotFound) => {}
                Err(error) => return Err(error.to_string()),
            }
            self.registered.remove(path);
        }
        self.watcher
            .watch(path, mode.to_notify())
            .map_err(|error| error.to_string())?;
        self.registered.insert(path.to_path_buf(), mode);
        Ok(true)
    }
}

fn clear_state(mut world: DeferredWorld, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .try_remove::<(Watching, WatchError, WatchReset)>();
}

fn start(world: &mut World) {
    if world.contains_non_send::<WatchDriver>() {
        return;
    }
    let Some(sender) = world
        .get_resource::<MessageSender<FileWatchEvent>>()
        .cloned()
    else {
        return;
    };
    match WatchDriver::new(sender) {
        Ok(driver) => {
            world.insert_non_send(driver);
        }
        Err(error) => tracing::warn!("file watcher init failed: {error}"),
    }
}

fn invalidate(
    mut events: MessageReader<FileWatchEvent>,
    watches: Query<(Entity, &FileWatch), With<Watching>>,
    mut commands: Commands,
) {
    let mut removed = HashSet::new();
    for event in events.read() {
        if !event.directory_removed {
            continue;
        }
        for path in &event.paths {
            removed.insert(PathIdentity::resolve(path).into_path_buf());
        }
    }
    if removed.is_empty() {
        return;
    }
    for (entity, watch) in &watches {
        let resolved = PathIdentity::resolve(watch.path()).into_path_buf();
        if removed.iter().any(|path| resolved.starts_with(path)) {
            commands.entity(entity).insert(WatchReset);
        }
    }
}

type WatchEntries<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, FileWatch>,
        Has<Watching>,
        Has<WatchReset>,
        Option<&'static WatchError>,
    ),
>;

fn reconcile(
    watches: WatchEntries,
    driver: Option<NonSendMut<WatchDriver>>,
    mut reconciled: MessageWriter<FileWatchReconciled>,
    mut commands: Commands,
) {
    let Some(mut driver) = driver else {
        return;
    };
    let mut desired: HashMap<PathBuf, (WatchMode, bool)> = HashMap::new();
    let mut groups: HashMap<PathBuf, Vec<Entity>> = HashMap::new();
    for (entity, watch, _, reset, _) in &watches {
        let path = PathIdentity::resolve(watch.path()).into_path_buf();
        let entry = desired.entry(path.clone()).or_insert((watch.mode(), false));
        if watch.mode() == WatchMode::Recursive {
            entry.0 = WatchMode::Recursive;
        }
        entry.1 |= reset;
        groups.entry(path).or_default().push(entity);
    }
    driver.unwatch_stale(&desired);
    for (path, (mode, reset)) in desired {
        let entities = &groups[&path];
        match driver.sync_path(&path, mode, reset) {
            Ok(synced) => {
                let mut became_watching = synced;
                for &entity in entities {
                    let Ok((_, watch, watching, entity_reset, error)) = watches.get(entity) else {
                        continue;
                    };
                    if !watching || entity_reset || watch.is_changed() {
                        became_watching = true;
                    }
                    if !watching || entity_reset || error.is_some() {
                        commands
                            .entity(entity)
                            .insert(Watching)
                            .remove::<(WatchError, WatchReset)>();
                    }
                }
                if became_watching {
                    reconciled.write(FileWatchReconciled { path });
                }
            }
            Err(message) => {
                let still_watching = driver.is_watching(&path);
                for &entity in entities {
                    let Ok((_, _, _, _, error)) = watches.get(entity) else {
                        continue;
                    };
                    if error.is_none_or(|current| current.message() != message) {
                        tracing::warn!(path = %path.display(), "file watch failed: {message}");
                        commands.entity(entity).insert(WatchError(message.clone()));
                    }
                    if !still_watching {
                        commands.entity(entity).remove::<(Watching, WatchReset)>();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::{Wake, Waker};
    use std::time::{Duration, Instant};

    use vmux_channel::ChannelWake;

    use super::*;

    struct TestWake(mpsc::Sender<()>);

    impl Wake for TestWake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    #[derive(Resource, Default)]
    struct RecordedEvents(Vec<FileWatchEvent>);

    #[derive(Resource, Default)]
    struct RecordedReconciled(Vec<PathBuf>);

    fn record_events(
        mut events: MessageReader<FileWatchEvent>,
        mut recorded: ResMut<RecordedEvents>,
    ) {
        for event in events.read() {
            recorded.0.push(event.clone());
        }
    }

    fn record_reconciled(
        mut events: MessageReader<FileWatchReconciled>,
        mut recorded: ResMut<RecordedReconciled>,
    ) {
        for event in events.read() {
            recorded.0.push(event.path.clone());
        }
    }

    struct WatchFixture {
        app: App,
        directory: tempfile::TempDir,
        wake: mpsc::Receiver<()>,
    }

    impl WatchFixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let (sender, wake) = mpsc::channel();
            let mut app = App::new();
            app.add_plugins((FileWatchPlugin, FileWatchPlugin))
                .insert_resource(ChannelWake(Waker::from(Arc::new(TestWake(sender)))))
                .init_resource::<RecordedEvents>()
                .init_resource::<RecordedReconciled>()
                .add_systems(
                    Update,
                    (record_events, record_reconciled).after(FileWatchSet::Reconcile),
                );
            Self {
                app,
                directory,
                wake,
            }
        }

        fn canonical_root(&self) -> PathBuf {
            PathIdentity::resolve(self.directory.path()).into_path_buf()
        }

        fn registered(&self) -> Vec<(PathBuf, WatchMode)> {
            let mut entries: Vec<_> = self
                .app
                .world()
                .non_send::<WatchDriver>()
                .registered
                .iter()
                .map(|(path, mode)| (path.clone(), *mode))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
        }

        fn discard_wakes(&self) {
            while self.wake.try_recv().is_ok() {}
        }

        fn wait_for_path(&mut self, expected: &Path) -> Vec<FileWatchEvent> {
            let expected = PathIdentity::resolve(expected).into_path_buf();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "timed out waiting for watch event on {}",
                    expected.display()
                );
                let _ = self
                    .wake
                    .recv_timeout(remaining.min(Duration::from_millis(50)));
                self.app.update();
                let recorded = &self.app.world().resource::<RecordedEvents>().0;
                if recorded
                    .iter()
                    .any(|event| event.paths.iter().any(|path| path == &expected))
                {
                    return recorded.clone();
                }
            }
        }
    }

    #[test]
    fn access_events_are_ignored_before_sending() {
        let access = notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Read))
            .add_path(PathBuf::from("/tmp/sample.txt"));
        assert!(FileWatchEvent::from_notify(Ok(access)).is_none());

        let create = notify::Event::new(notify::EventKind::Create(notify::event::CreateKind::File))
            .add_path(PathBuf::from("/tmp/sample.txt"));
        let converted = FileWatchEvent::from_notify(Ok(create)).unwrap();
        assert_eq!(converted.kind, FileWatchEventKind::Created);
        assert!(!converted.directory_removed);
        assert!(converted.error.is_none());

        let remove =
            notify::Event::new(notify::EventKind::Remove(notify::event::RemoveKind::Folder))
                .add_path(PathBuf::from("/tmp/removed"));
        let converted = FileWatchEvent::from_notify(Ok(remove)).unwrap();
        assert_eq!(converted.kind, FileWatchEventKind::Removed);
        assert!(converted.directory_removed);

        let rename = notify::Event::new(notify::EventKind::Modify(
            notify::event::ModifyKind::Name(notify::event::RenameMode::From),
        ))
        .add_path(PathBuf::from("/tmp/renamed"));
        let converted = FileWatchEvent::from_notify(Ok(rename)).unwrap();
        assert_eq!(converted.kind, FileWatchEventKind::Renamed);
        assert!(converted.directory_removed);
    }

    #[test]
    fn shared_watches_reference_count_and_upgrade_or_downgrade_mode() {
        let mut fixture = WatchFixture::new();
        let root = fixture.canonical_root();

        let first = fixture
            .app
            .world_mut()
            .spawn(FileWatch::non_recursive(&root))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(first).is_some());
        assert_eq!(
            fixture.registered(),
            [(root.clone(), WatchMode::NonRecursive)]
        );
        assert_eq!(
            fixture.app.world().resource::<RecordedReconciled>().0,
            std::slice::from_ref(&root)
        );

        let second = fixture
            .app
            .world_mut()
            .spawn(FileWatch::recursive(&root))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(second).is_some());
        assert_eq!(fixture.registered(), [(root.clone(), WatchMode::Recursive)]);

        fixture.app.world_mut().despawn(second);
        fixture.app.update();
        assert_eq!(
            fixture.registered(),
            [(root.clone(), WatchMode::NonRecursive)]
        );

        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .remove::<FileWatch>();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(first).is_none());
        assert!(fixture.registered().is_empty());
    }

    #[test]
    fn missing_path_records_watch_error_and_reconciles_once_created() {
        let mut fixture = WatchFixture::new();
        let missing = fixture.directory.path().join("later");
        let entity = fixture
            .app
            .world_mut()
            .spawn(FileWatch::non_recursive(&missing))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(entity).is_none());
        let error = fixture.app.world().get::<WatchError>(entity).unwrap();
        assert!(!error.message().is_empty());

        std::fs::create_dir(&missing).unwrap();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(entity).is_some());
        assert!(fixture.app.world().get::<WatchError>(entity).is_none());
        let expected = PathIdentity::resolve(&missing).into_path_buf();
        assert_eq!(
            fixture.app.world().resource::<RecordedReconciled>().0,
            [expected]
        );
    }

    #[test]
    fn directory_removed_event_resets_and_reconciles_active_watch() {
        let mut fixture = WatchFixture::new();
        let root = fixture.canonical_root();
        let entity = fixture
            .app
            .world_mut()
            .spawn(FileWatch::non_recursive(&root))
            .id();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .resource_mut::<RecordedReconciled>()
            .0
            .clear();

        fixture
            .app
            .world_mut()
            .write_message(FileWatchEvent::directory_removed(vec![root.clone()]));
        fixture.app.update();

        assert!(fixture.app.world().get::<Watching>(entity).is_some());
        assert!(fixture.app.world().get::<WatchReset>(entity).is_none());
        assert_eq!(
            fixture.app.world().resource::<RecordedReconciled>().0,
            [root]
        );
    }

    #[test]
    fn native_filesystem_changes_deliver_events_and_wake_the_channel() {
        let mut fixture = WatchFixture::new();
        let root = fixture.canonical_root();
        let entity = fixture
            .app
            .world_mut()
            .spawn(FileWatch::non_recursive(&root))
            .id();
        fixture.app.update();
        assert!(fixture.app.world().get::<Watching>(entity).is_some());
        fixture.discard_wakes();

        let file = fixture.directory.path().join("hello.txt");
        std::fs::write(&file, "v1\n").unwrap();
        let events = fixture.wait_for_path(&file);
        assert!(!events.is_empty());

        fixture
            .app
            .world_mut()
            .resource_mut::<RecordedEvents>()
            .0
            .clear();
        fixture.discard_wakes();
        std::fs::write(&file, "v2\n").unwrap();
        let events = fixture.wait_for_path(&file);
        assert!(!events.is_empty());

        fixture
            .app
            .world_mut()
            .resource_mut::<RecordedEvents>()
            .0
            .clear();
        fixture.discard_wakes();
        std::fs::remove_file(&file).unwrap();
        let events = fixture.wait_for_path(&file);
        assert!(!events.is_empty());

        fixture.app.world_mut().despawn(entity);
        fixture.app.update();
        assert!(fixture.registered().is_empty());
    }
}
