use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{
    EntityInbox, EntityInboxPlugin, EntityInboxReceived, InboxClosed, InboxPlugin,
    InboxSendErrorKind, JobWake, MessageBridge, SpawnBridge, SpawnInboxPlugin,
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

struct ConsumerInboxPlugin;

impl Plugin for ConsumerInboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            InboxPlugin::<NoteEvent>::bounded(2),
            SpawnInboxPlugin::<WorkOrder>::default(),
            EntityInboxPlugin::<String>::default(),
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

fn collect_chunk(
    mut trigger: On<EntityInboxReceived<String>>,
    mut chunks: ResMut<ObservedChunks>,
) {
    let entity = trigger.event().entity;
    let Some(chunk) = trigger.event_mut().take() else {
        return;
    };
    chunks.0.push((entity, chunk));
}

#[test]
fn consumer_uses_message_spawn_and_entity_inboxes_through_job_wake() {
    let mut app = App::new();
    let wake = WakeCounter::new();
    app.add_plugins(ConsumerInboxPlugin)
        .insert_resource(JobWake(Waker::from(Arc::clone(&wake))));
    app.update();

    let notes = app.world().resource::<MessageBridge<NoteEvent>>().clone();
    let orders = app.world().resource::<SpawnBridge<WorkOrder>>().clone();
    let (stream_tx, stream_rx) = EntityInbox::<String>::unbounded();
    let worker = app.world_mut().spawn(stream_rx).id();
    app.update();

    notes.send(NoteEvent("first".into())).unwrap();
    notes.send(NoteEvent("second".into())).unwrap();
    assert_eq!(
        notes.send(NoteEvent("overflow".into())).unwrap_err().kind(),
        InboxSendErrorKind::Full
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
        [(worker, "chunk-a".to_string()), (worker, "chunk-b".to_string())]
    );
    assert!(app.world().get::<InboxClosed<String>>(worker).is_some());
}
