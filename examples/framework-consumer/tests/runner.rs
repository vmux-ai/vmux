use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy_app::{App, AppExit, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::channel::{AsyncChannelPlugin, MessageSender};
use vmux_app::ecs::time::{Clock, Deadline, DeadlinePlugin, DeadlineReached};
use vmux_app::ecs::{
    Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownPhase, ShutdownPlugin, ShutdownRequest,
    ShutdownState,
};
use vmux_app::runner::HeadlessTokioRunner;

#[derive(Message, Clone)]
struct IncomingCommand(String);

struct FlushTag;

struct ReadyTag;

#[derive(Component)]
struct DrainGuard;

#[derive(Resource, Default)]
struct ConsumerTrace {
    received: Vec<String>,
    ready: bool,
    ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

fn on_ready(_: On<DeadlineReached<ReadyTag>>, mut trace: ResMut<ConsumerTrace>) {
    trace.ready = true;
    trace.events.lock().unwrap().push("deadline");
    if let Some(tx) = trace.ready_tx.take() {
        let _ = tx.send(());
    }
}

fn on_incoming(
    mut reader: MessageReader<IncomingCommand>,
    mut trace: ResMut<ConsumerTrace>,
    mut shutdown: MessageWriter<ShutdownRequest>,
) {
    for command in reader.read() {
        trace.received.push(command.0.clone());
        trace.events.lock().unwrap().push("channel");
    }
    if trace.ready && !trace.received.is_empty() {
        shutdown.write(ShutdownRequest::success());
    }
}

fn begin_drain(
    mut commands: Commands,
    clock: Res<Clock>,
    state: Res<ShutdownState>,
    trace: Res<ConsumerTrace>,
) {
    assert_eq!(state.phase(), ShutdownPhase::Draining);
    trace.events.lock().unwrap().push("shutdown");
    commands.spawn((
        DrainGuard,
        ShutdownBarrier,
        Deadline::<FlushTag>::after(&clock, Duration::from_millis(20)),
    ));
}

fn finish_drain(
    reached: On<DeadlineReached<FlushTag>>,
    mut commands: Commands,
    trace: Res<ConsumerTrace>,
) {
    trace.events.lock().unwrap().push("drained");
    commands.entity(reached.entity).remove::<ShutdownBarrier>();
}

fn record_complete(state: Res<ShutdownState>, trace: Res<ConsumerTrace>) {
    assert_eq!(state.phase(), ShutdownPhase::Complete);
    trace.events.lock().unwrap().push("complete");
}

#[test]
fn headless_tokio_runner_composes_channels_deadlines_and_shutdown_without_desktop_crates() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().jobs(true).build(),
        ShutdownPlugin,
        AsyncChannelPlugin::message::<IncomingCommand>(),
        DeadlinePlugin::<ReadyTag>::default(),
        DeadlinePlugin::<FlushTag>::default(),
    ))
    .insert_resource(ConsumerTrace {
        ready_tx: Some(ready_tx),
        events: Arc::clone(&events),
        ..Default::default()
    })
    .add_observer(on_ready)
    .add_observer(finish_drain)
    .add_systems(Update, on_incoming)
    .add_systems(Shutdown, begin_drain)
    .add_systems(ShutdownComplete, record_complete)
    .set_runner(
        HeadlessTokioRunner::new(runtime.handle().clone())
            .with_housekeeping_floor(Duration::from_secs(30))
            .into_runner(),
    );

    let clock = app.world().resource::<Clock>().clone();
    app.world_mut().spawn(Deadline::<ReadyTag>::after(
        &clock,
        Duration::from_millis(15),
    ));
    let sender = app
        .world()
        .resource::<MessageSender<IncomingCommand>>()
        .clone();
    runtime.spawn(async move {
        ready_rx.await.unwrap();
        sender.send(IncomingCommand("sync".into())).unwrap();
    });

    let started = Instant::now();
    assert_eq!(app.run(), AppExit::Success);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "headless runner should wake immediately instead of waiting on the 30s housekeeping floor"
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec!["deadline", "channel", "shutdown", "drained", "complete"]
    );
}
