use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{
    AsyncChannel, AsyncChannelPlugin, ChannelClosed, ChannelReceived, ChannelSendErrorKind,
    JobWake, MessageSender, SpawnSender,
};

struct WakeCounter(AtomicUsize);

impl WakeCounter {
    fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(0)))
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

impl std::task::Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
struct NoteEvent(String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct WorkOrder(u32);

#[derive(Resource, Default)]
struct ObservedNotes(Vec<String>);

#[derive(Resource, Default)]
struct ObservedChunks(Vec<(Entity, String)>);

struct ConsumerChannelPlugin;

impl Plugin for ConsumerChannelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            AsyncChannelPlugin::message::<NoteEvent>().bounded(2),
            AsyncChannelPlugin::spawn::<WorkOrder>(),
            AsyncChannelPlugin::<String>::default(),
        ))
        .init_resource::<ObservedNotes>()
        .init_resource::<ObservedChunks>()
        .add_systems(Update, collect_notes)
        .add_observer(collect_chunk);
    }
}

fn collect_notes(mut reader: MessageReader<NoteEvent>, mut notes: ResMut<ObservedNotes>) {
    for note in reader.read() {
        notes.0.push(note.0.clone());
    }
}

fn collect_chunk(mut trigger: On<ChannelReceived<String>>, mut chunks: ResMut<ObservedChunks>) {
    let entity = trigger.event().entity;
    let Some(chunk) = trigger.event_mut().take() else {
        return;
    };
    chunks.0.push((entity, chunk));
}

#[test]
fn consumer_uses_message_spawn_and_entity_channels_through_job_wake() {
    let mut app = App::new();
    let wake = WakeCounter::new();
    app.add_plugins(ConsumerChannelPlugin)
        .insert_resource(JobWake(Waker::from(Arc::clone(&wake))));
    app.update();

    let notes = app.world().resource::<MessageSender<NoteEvent>>().clone();
    let orders = app.world().resource::<SpawnSender<WorkOrder>>().clone();
    let (stream_tx, stream_rx) = AsyncChannel::<String>::unbounded();
    let worker = app.world_mut().spawn(stream_rx).id();
    app.update();

    notes.send(NoteEvent("first".into())).unwrap();
    notes.send(NoteEvent("second".into())).unwrap();
    assert_eq!(
        notes.send(NoteEvent("overflow".into())).unwrap_err().kind(),
        ChannelSendErrorKind::Full
    );
    orders.send(WorkOrder(42)).unwrap();
    stream_tx.send("chunk-a".into()).unwrap();
    stream_tx.send("chunk-b".into()).unwrap();
    drop(stream_tx);
    assert!(wake.count() >= 5);

    app.update();

    assert_eq!(
        app.world().resource::<ObservedNotes>().0,
        ["first", "second"]
    );
    let spawned = app
        .world_mut()
        .query::<&WorkOrder>()
        .iter(app.world())
        .map(|order| order.0)
        .collect::<Vec<_>>();
    assert_eq!(spawned, [42]);
    assert_eq!(
        app.world().resource::<ObservedChunks>().0,
        [
            (worker, "chunk-a".to_string()),
            (worker, "chunk-b".to_string())
        ]
    );
    assert!(app.world().get::<ChannelClosed<String>>(worker).is_some());
}
