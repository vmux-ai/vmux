use std::future::pending;
use std::sync::{Arc, mpsc};
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::ecs::{IoTask, IoTaskPlugin, JobWake, TaskError};

#[derive(Component, Debug, PartialEq, Eq)]
struct Loaded(u32);

#[derive(Component, Debug, PartialEq, Eq)]
struct Secondary(u32);

struct Notify(mpsc::SyncSender<()>);

impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.try_send(());
    }
}

struct DropNotice(mpsc::SyncSender<()>);

impl Drop for DropNotice {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

struct Consumer {
    app: App,
}

impl Consumer {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            IoTaskPlugin::<(Loaded, Secondary)>::default(),
            IoTaskPlugin::<(Loaded,)>::default(),
        ));
        Self { app }
    }

    fn wait_until(&mut self, check: impl Fn(&World) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.app.update();
            if check(self.app.world()) {
                return;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}

#[test]
fn io_task_plugin_delivers_ok_error_and_panic_through_vmux_app_ecs() {
    let mut consumer = Consumer::new();
    let entity = consumer
        .app
        .world_mut()
        .spawn(IoTask::spawn(async { Ok::<_, String>(Loaded(42)) }))
        .id();
    consumer.wait_until(|world| world.get::<Loaded>(entity).is_some());
    assert_eq!(
        consumer.app.world().get::<Loaded>(entity),
        Some(&Loaded(42))
    );
    assert!(
        consumer
            .app
            .world()
            .get::<IoTask<Loaded>>(entity)
            .is_none()
    );

    consumer
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(IoTask::spawn(async {
            Err::<Loaded, String>("network failed".into())
        }));
    consumer.wait_until(|world| world.get::<TaskError<Loaded>>(entity).is_some());
    let error = consumer
        .app
        .world()
        .get::<TaskError<Loaded>>(entity)
        .unwrap();
    assert!(!error.is_panic());
    assert_eq!(error.error().map(String::as_str), Some("network failed"));
    assert_eq!(error.to_string(), "network failed");

    consumer
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(IoTask::<Loaded>::spawn(async {
            panic!("consumer task panic");
        }));
    assert!(
        consumer
            .app
            .world()
            .get::<TaskError<Loaded>>(entity)
            .is_none()
    );
    consumer.wait_until(|world| world.get::<TaskError<Loaded>>(entity).is_some());
    let panic_error = consumer
        .app
        .world()
        .get::<TaskError<Loaded>>(entity)
        .unwrap();
    assert!(panic_error.is_panic());
    assert_eq!(panic_error.error(), None);
}

#[test]
fn job_wake_automatically_wakes_io_tasks_on_registration_and_completion() {
    let mut consumer = Consumer::new();
    let (wake_tx, wake_rx) = mpsc::sync_channel(8);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    consumer
        .app
        .insert_resource(JobWake(Waker::from(Arc::new(Notify(wake_tx)))));
    let entity = consumer
        .app
        .world_mut()
        .spawn(IoTask::spawn(async move {
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok::<_, String>(Loaded(9))
        }))
        .id();
    wake_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    consumer.app.update();
    assert!(wake_rx.try_recv().is_err());
    assert!(consumer.app.world().get::<Loaded>(entity).is_none());
    release_tx.send(()).unwrap();
    wake_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    consumer.app.update();
    assert_eq!(consumer.app.world().get::<Loaded>(entity), Some(&Loaded(9)));
}

#[test]
fn replacement_and_despawn_cancel_in_flight_io_tasks() {
    let mut consumer = Consumer::new();
    let (first_started_tx, first_started_rx) = mpsc::sync_channel(1);
    let (first_dropped_tx, first_dropped_rx) = mpsc::sync_channel(1);
    let entity = consumer
        .app
        .world_mut()
        .spawn(IoTask::spawn(async move {
            let _notice = DropNotice(first_dropped_tx);
            first_started_tx.send(()).unwrap();
            pending::<Result<Loaded, String>>().await
        }))
        .id();
    first_started_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap();

    let (second_started_tx, second_started_rx) = mpsc::sync_channel(1);
    let (second_dropped_tx, second_dropped_rx) = mpsc::sync_channel(1);
    consumer
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(IoTask::spawn(async move {
            let _notice = DropNotice(second_dropped_tx);
            second_started_tx.send(()).unwrap();
            pending::<Result<Loaded, String>>().await
        }));
    first_dropped_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    second_started_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap();

    consumer.app.world_mut().entity_mut(entity).despawn();
    second_dropped_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
}

#[test]
fn tuple_io_task_plugin_registers_multiple_components_idempotently() {
    let mut consumer = Consumer::new();
    let first = consumer
        .app
        .world_mut()
        .spawn(IoTask::spawn(async { Ok::<_, String>(Loaded(1)) }))
        .id();
    let second = consumer
        .app
        .world_mut()
        .spawn(IoTask::spawn(async { Ok::<_, String>(Secondary(2)) }))
        .id();
    consumer.wait_until(|world| {
        world.get::<Loaded>(first).is_some() && world.get::<Secondary>(second).is_some()
    });
    assert_eq!(consumer.app.world().get::<Loaded>(first), Some(&Loaded(1)));
    assert_eq!(
        consumer.app.world().get::<Secondary>(second),
        Some(&Secondary(2))
    );
}
