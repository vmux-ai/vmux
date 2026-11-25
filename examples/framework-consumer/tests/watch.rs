use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::task::Waker;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::JobWake;
use vmux_app::ecs::watch::{
    FileWatch, FileWatchEvent, FileWatchPlugin, FileWatchReconciled, FileWatchSet, Watching,
};

struct WakeSignal {
    count: AtomicUsize,
    sender: mpsc::Sender<()>,
}

impl WakeSignal {
    fn new() -> (Arc<Self>, mpsc::Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        (
            Arc::new(Self {
                count: AtomicUsize::new(0),
                sender,
            }),
            receiver,
        )
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }
}

impl std::task::Wake for WakeSignal {
    fn wake(self: Arc<Self>) {
        self.count.fetch_add(1, Ordering::AcqRel);
        let _ = self.sender.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.count.fetch_add(1, Ordering::AcqRel);
        let _ = self.sender.send(());
    }
}

#[derive(Resource, Default)]
struct ObservedPaths(Vec<PathBuf>);

#[derive(Resource, Default)]
struct ReconciledPaths(Vec<PathBuf>);

struct ConsumerWatchPlugin;

impl Plugin for ConsumerWatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((VmuxPlugin::builder().jobs(true).build(), FileWatchPlugin))
            .init_resource::<ObservedPaths>()
            .init_resource::<ReconciledPaths>()
            .add_systems(
                Update,
                (collect_events, collect_reconciled).after(FileWatchSet::Reconcile),
            );
    }
}

fn collect_events(
    mut reader: MessageReader<FileWatchEvent>,
    mut observed: ResMut<ObservedPaths>,
) {
    for event in reader.read() {
        observed.0.extend(event.paths.iter().cloned());
    }
}

fn collect_reconciled(
    mut reader: MessageReader<FileWatchReconciled>,
    mut reconciled: ResMut<ReconciledPaths>,
) {
    for event in reader.read() {
        reconciled.0.push(event.path.clone());
    }
}

fn wait_for_path(app: &mut App, receiver: &mpsc::Receiver<()>, target: &Path) {
    let canonical = target
        .canonicalize()
        .unwrap_or_else(|_| target.parent().unwrap().canonicalize().unwrap().join(target.file_name().unwrap()));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for watch event on {}",
            canonical.display()
        );
        let _ = receiver.recv_timeout(remaining.min(Duration::from_millis(50)));
        app.update();
        if app
            .world()
            .resource::<ObservedPaths>()
            .0
            .iter()
            .any(|path| path == &canonical)
        {
            return;
        }
    }
}

#[test]
fn consumer_watches_filesystem_changes_with_job_wake_and_unwatches_on_despawn() {
    let directory = tempfile::tempdir().unwrap();
    let canonical_root = directory.path().canonicalize().unwrap();
    let (wake, receiver) = WakeSignal::new();

    let mut app = App::new();
    app.add_plugins(ConsumerWatchPlugin)
        .insert_resource(JobWake(Waker::from(Arc::clone(&wake))));

    let watch = app
        .world_mut()
        .spawn(FileWatch::non_recursive(directory.path()))
        .id();
    app.update();

    assert!(app.world().get::<Watching>(watch).is_some());
    assert_eq!(
        app.world().resource::<ReconciledPaths>().0,
        [canonical_root]
    );

    while receiver.try_recv().is_ok() {}
    let wakes_before = wake.count();

    let file = directory.path().join("note.txt");
    std::fs::write(&file, "created\n").unwrap();
    wait_for_path(&mut app, &receiver, &file);
    assert!(wake.count() > wakes_before);

    app.world_mut().resource_mut::<ObservedPaths>().0.clear();
    while receiver.try_recv().is_ok() {}
    let wakes_before_modify = wake.count();
    std::fs::write(&file, "modified\n").unwrap();
    wait_for_path(&mut app, &receiver, &file);
    assert!(wake.count() > wakes_before_modify);

    app.world_mut().resource_mut::<ObservedPaths>().0.clear();
    while receiver.try_recv().is_ok() {}
    let wakes_before_remove = wake.count();
    std::fs::remove_file(&file).unwrap();
    wait_for_path(&mut app, &receiver, &file);
    assert!(wake.count() > wakes_before_remove);

    app.world_mut().despawn(watch);
    app.update();

    app.world_mut().resource_mut::<ObservedPaths>().0.clear();
    while receiver.try_recv().is_ok() {}
    let after_unwatch = directory.path().join("ignored.txt");
    std::fs::write(&after_unwatch, "after despawn\n").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    app.update();
    assert!(
        !app.world()
            .resource::<ObservedPaths>()
            .0
            .iter()
            .any(|path| path.ends_with("ignored.txt"))
    );
}
