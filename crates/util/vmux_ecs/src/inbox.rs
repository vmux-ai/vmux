use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use bevy_app::{App, Plugin, PreUpdate};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;

#[cfg(feature = "jobs")]
use crate::JobWake;

pub struct InboxPlugin<M: Message> {
    capacity: Option<usize>,
    marker: PhantomData<fn() -> M>,
}

impl<M: Message> Default for InboxPlugin<M> {
    fn default() -> Self {
        Self {
            capacity: None,
            marker: PhantomData,
        }
    }
}

impl<M: Message> InboxPlugin<M> {
    pub fn bounded(capacity: usize) -> Self {
        Self {
            capacity: Some(capacity),
            marker: PhantomData,
        }
    }
}

impl<M: Message> Plugin for InboxPlugin<M> {
    fn build(&self, app: &mut App) {
        app.add_message::<M>()
            .configure_sets(PreUpdate, (InboxSet::BindWake, InboxSet::Drain).chain());
        if !app.world().contains_resource::<MessageInbox<M>>()
            && !app.world().contains_resource::<MessageBridge<M>>()
        {
            let (bridge, inbox) = match self.capacity {
                Some(capacity) => MessageInbox::<M>::bounded(capacity),
                None => MessageInbox::<M>::unbounded(),
            };
            app.insert_resource(bridge).insert_resource(inbox);
        }
        #[cfg(feature = "jobs")]
        app.add_systems(PreUpdate, bind_message_wake::<M>.in_set(InboxSet::BindWake));
        app.add_systems(PreUpdate, drain_messages::<M>.in_set(InboxSet::Drain));
    }
}

pub struct SpawnInboxPlugin<B: Bundle> {
    capacity: Option<usize>,
    resource: bool,
    marker: PhantomData<fn() -> B>,
}

impl<B: Bundle> Default for SpawnInboxPlugin<B> {
    fn default() -> Self {
        Self {
            capacity: None,
            resource: true,
            marker: PhantomData,
        }
    }
}

impl<B: Bundle> SpawnInboxPlugin<B> {
    pub fn bounded(capacity: usize) -> Self {
        Self {
            capacity: Some(capacity),
            resource: true,
            marker: PhantomData,
        }
    }

    pub fn components() -> Self {
        Self {
            capacity: None,
            resource: false,
            marker: PhantomData,
        }
    }
}

impl<B: Bundle> Plugin for SpawnInboxPlugin<B> {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, (InboxSet::BindWake, InboxSet::Drain).chain());
        if self.resource
            && !app.world().contains_resource::<SpawnInbox<B>>()
            && !app.world().contains_resource::<SpawnBridge<B>>()
        {
            let (bridge, inbox) = match self.capacity {
                Some(capacity) => SpawnInbox::<B>::bounded(capacity),
                None => SpawnInbox::<B>::unbounded(),
            };
            app.insert_resource(bridge).insert_resource(inbox);
        }
        #[cfg(feature = "jobs")]
        app.add_systems(PreUpdate, bind_spawn_wake::<B>.in_set(InboxSet::BindWake));
        app.add_systems(PreUpdate, drain_spawns::<B>.in_set(InboxSet::Drain));
    }
}

pub struct EntityInboxPlugin<T: Send + Sync + 'static>(PhantomData<fn() -> T>);

impl<T: Send + Sync + 'static> Default for EntityInboxPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for EntityInboxPlugin<T> {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, (InboxSet::BindWake, InboxSet::Drain).chain());
        #[cfg(feature = "jobs")]
        app.add_systems(PreUpdate, bind_entity_wake::<T>.in_set(InboxSet::BindWake));
        app.add_systems(PreUpdate, drain_entities::<T>.in_set(InboxSet::Drain));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InboxSet {
    BindWake,
    Drain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxSendErrorKind {
    Full,
    Closed,
}

#[derive(PartialEq, Eq)]
pub struct InboxSendError<T> {
    kind: InboxSendErrorKind,
    value: T,
}

impl<T> InboxSendError<T> {
    pub fn kind(&self) -> InboxSendErrorKind {
        self.kind
    }

    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> fmt::Debug for InboxSendError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InboxSendError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl<T> fmt::Display for InboxSendError<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            InboxSendErrorKind::Full => "inbox channel is full",
            InboxSendErrorKind::Closed => "inbox channel is closed",
        })
    }
}

impl<T> std::error::Error for InboxSendError<T> {}

#[derive(Resource)]
pub struct MessageBridge<M: Message> {
    sender: ChannelSender<M>,
}

impl<M: Message> Clone for MessageBridge<M> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<M: Message> MessageBridge<M> {
    pub fn send(&self, message: M) -> Result<(), InboxSendError<M>> {
        self.sender.send(message)
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub fn bind_waker(&self, waker: Waker) {
        self.sender.bind_custom_waker(waker);
    }
}

#[derive(Resource)]
pub struct MessageInbox<M: Message> {
    receiver: ChannelReceiver<M>,
}

impl<M: Message> MessageInbox<M> {
    pub fn unbounded() -> (MessageBridge<M>, Self) {
        let (sender, receiver) = Channel::open(None, None);
        (MessageBridge { sender }, Self { receiver })
    }

    pub fn bounded(capacity: usize) -> (MessageBridge<M>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), None);
        (MessageBridge { sender }, Self { receiver })
    }

    pub fn unbounded_with_waker(waker: Waker) -> (MessageBridge<M>, Self) {
        let (sender, receiver) = Channel::open(None, Some(waker));
        (MessageBridge { sender }, Self { receiver })
    }

    pub fn bounded_with_waker(capacity: usize, waker: Waker) -> (MessageBridge<M>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), Some(waker));
        (MessageBridge { sender }, Self { receiver })
    }

    pub fn close(&self) {
        self.receiver.close();
    }

    pub fn close_and_clear(&self) {
        self.receiver.close_and_clear();
    }

    pub fn is_disconnected(&self) -> bool {
        self.receiver.is_disconnected()
    }
}

#[derive(Resource)]
pub struct SpawnBridge<B: Bundle> {
    sender: ChannelSender<B>,
}

impl<B: Bundle> Clone for SpawnBridge<B> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<B: Bundle> SpawnBridge<B> {
    pub fn send(&self, bundle: B) -> Result<(), InboxSendError<B>> {
        self.sender.send(bundle)
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub fn bind_waker(&self, waker: Waker) {
        self.sender.bind_custom_waker(waker);
    }
}

#[derive(Resource)]
pub struct SpawnInbox<B: Bundle> {
    receiver: ChannelReceiver<B>,
}

impl<B: Bundle> SpawnInbox<B> {
    pub fn unbounded() -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(None, None);
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn bounded(capacity: usize) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), None);
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn unbounded_with_waker(waker: Waker) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(None, Some(waker));
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn bounded_with_waker(capacity: usize, waker: Waker) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), Some(waker));
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn close(&self) {
        self.receiver.close();
    }

    pub fn close_and_clear(&self) {
        self.receiver.close_and_clear();
    }

    pub fn is_disconnected(&self) -> bool {
        self.receiver.is_disconnected()
    }
}

#[derive(Component)]
pub struct EntitySpawnInbox<B: Bundle> {
    receiver: ChannelReceiver<B>,
}

impl<B: Bundle> EntitySpawnInbox<B> {
    pub fn unbounded() -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(None, None);
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn bounded(capacity: usize) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), None);
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn unbounded_with_waker(waker: Waker) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(None, Some(waker));
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn bounded_with_waker(capacity: usize, waker: Waker) -> (SpawnBridge<B>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), Some(waker));
        (SpawnBridge { sender }, Self { receiver })
    }

    pub fn close(&self) {
        self.receiver.close();
    }

    pub fn close_and_clear(&self) {
        self.receiver.close_and_clear();
    }

    pub fn is_disconnected(&self) -> bool {
        self.receiver.is_disconnected()
    }
}

pub struct EntityBridge<T: Send + Sync + 'static> {
    sender: ChannelSender<T>,
}

impl<T: Send + Sync + 'static> Clone for EntityBridge<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Send + Sync + 'static> Drop for EntityBridge<T> {
    fn drop(&mut self) {}
}

impl<T: Send + Sync + 'static> EntityBridge<T> {
    pub fn send(&self, item: T) -> Result<(), InboxSendError<T>> {
        self.sender.send(item)
    }

    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    pub fn bind_waker(&self, waker: Waker) {
        self.sender.bind_custom_waker(waker);
    }
}

#[derive(Component)]
#[component(on_insert = clear_closed::<T>)]
pub struct EntityInbox<T: Send + Sync + 'static> {
    receiver: ChannelReceiver<T>,
}

impl<T: Send + Sync + 'static> EntityInbox<T> {
    pub fn unbounded() -> (EntityBridge<T>, Self) {
        let (sender, receiver) = Channel::open(None, None);
        (EntityBridge { sender }, Self { receiver })
    }

    pub fn bounded(capacity: usize) -> (EntityBridge<T>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), None);
        (EntityBridge { sender }, Self { receiver })
    }

    pub fn unbounded_with_waker(waker: Waker) -> (EntityBridge<T>, Self) {
        let (sender, receiver) = Channel::open(None, Some(waker));
        (EntityBridge { sender }, Self { receiver })
    }

    pub fn bounded_with_waker(capacity: usize, waker: Waker) -> (EntityBridge<T>, Self) {
        let (sender, receiver) = Channel::open(Some(capacity), Some(waker));
        (EntityBridge { sender }, Self { receiver })
    }

    pub fn try_recv(&self) -> Option<T> {
        self.receiver.try_recv()
    }

    pub fn drain(&self) -> Vec<T> {
        self.receiver.drain_all().0
    }

    pub fn close(&self) {
        self.receiver.close();
    }

    pub fn close_and_clear(&self) {
        self.receiver.close_and_clear();
    }

    pub fn is_disconnected(&self) -> bool {
        self.receiver.is_disconnected()
    }
}

#[derive(EntityEvent)]
pub struct EntityInboxReceived<T: Send + Sync + 'static> {
    #[event_target]
    pub entity: Entity,
    item: Option<T>,
}

impl<T: Send + Sync + 'static> EntityInboxReceived<T> {
    pub fn get(&self) -> Option<&T> {
        self.item.as_ref()
    }

    pub fn take(&mut self) -> Option<T> {
        self.item.take()
    }
}

#[derive(Component)]
pub struct InboxClosed<T: Send + Sync + 'static>(PhantomData<fn() -> T>);

impl<T: Send + Sync + 'static> Default for InboxClosed<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Clone for InboxClosed<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for InboxClosed<T> {}

impl<T: Send + Sync + 'static> fmt::Debug for InboxClosed<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InboxClosed")
    }
}

impl<T: Send + Sync + 'static> PartialEq for InboxClosed<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T: Send + Sync + 'static> Eq for InboxClosed<T> {}

#[allow(unknown_lints, vmux_ecs_ownership)]
struct Channel<T> {
    state: Mutex<Queue<T>>,
    wake: Mutex<WakeState>,
}

struct Queue<T> {
    items: VecDeque<T>,
    capacity: Option<usize>,
    senders: usize,
    closed: bool,
}

struct WakeState {
    waker: Option<Waker>,
    custom: bool,
}

impl<T> Channel<T> {
    fn open(
        capacity: Option<usize>,
        waker: Option<Waker>,
    ) -> (ChannelSender<T>, ChannelReceiver<T>) {
        let custom = waker.is_some();
        let channel = Arc::new(Self {
            state: Mutex::new(Queue {
                items: VecDeque::new(),
                capacity,
                senders: 1,
                closed: false,
            }),
            wake: Mutex::new(WakeState { waker, custom }),
        });
        (
            ChannelSender(Arc::clone(&channel)),
            ChannelReceiver(channel),
        )
    }

    fn wake(&self) {
        let waker = {
            let Ok(wake) = self.wake.lock() else {
                return;
            };
            wake.waker.clone()
        };
        if let Some(waker) = waker {
            let _ = catch_unwind(AssertUnwindSafe(|| waker.wake()));
        }
    }

    fn has_pending_work(&self) -> bool {
        let Ok(queue) = self.state.lock() else {
            return false;
        };
        !queue.items.is_empty() || (!queue.closed && queue.senders == 0)
    }

    fn bind_custom_waker(&self, waker: Waker) {
        {
            let Ok(mut wake) = self.wake.lock() else {
                return;
            };
            wake.waker = Some(waker);
            wake.custom = true;
        }
        if self.has_pending_work() {
            self.wake();
        }
    }

    #[cfg(feature = "jobs")]
    fn bind_auto_waker(&self, waker: &Waker) {
        let updated = {
            let Ok(mut wake) = self.wake.lock() else {
                return;
            };
            if wake.custom {
                return;
            }
            if wake
                .waker
                .as_ref()
                .is_some_and(|current| current.will_wake(waker))
            {
                false
            } else {
                wake.waker = Some(waker.clone());
                true
            }
        };
        if updated && self.has_pending_work() {
            self.wake();
        }
    }
}

struct ChannelSender<T>(Arc<Channel<T>>);

impl<T> Clone for ChannelSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut queue) = self.0.state.lock() {
            queue.senders = queue.senders.saturating_add(1);
        }
        Self(Arc::clone(&self.0))
    }
}

impl<T> Drop for ChannelSender<T> {
    fn drop(&mut self) {
        let disconnected = {
            let Ok(mut queue) = self.0.state.lock() else {
                return;
            };
            queue.senders = queue.senders.saturating_sub(1);
            queue.senders == 0 && !queue.closed
        };
        if disconnected {
            self.0.wake();
        }
    }
}

impl<T> ChannelSender<T> {
    fn send(&self, value: T) -> Result<(), InboxSendError<T>> {
        let result = {
            let Ok(mut queue) = self.0.state.lock() else {
                return Err(InboxSendError {
                    kind: InboxSendErrorKind::Closed,
                    value,
                });
            };
            if queue.closed {
                Err(InboxSendError {
                    kind: InboxSendErrorKind::Closed,
                    value,
                })
            } else if queue
                .capacity
                .is_some_and(|capacity| queue.items.len() >= capacity)
            {
                Err(InboxSendError {
                    kind: InboxSendErrorKind::Full,
                    value,
                })
            } else {
                queue.items.push_back(value);
                Ok(())
            }
        };
        if result.is_ok() {
            self.0.wake();
        }
        result
    }

    fn is_closed(&self) -> bool {
        let Ok(queue) = self.0.state.lock() else {
            return true;
        };
        queue.closed
    }

    fn bind_custom_waker(&self, waker: Waker) {
        self.0.bind_custom_waker(waker);
    }
}

struct ChannelReceiver<T>(Arc<Channel<T>>);

impl<T> Drop for ChannelReceiver<T> {
    fn drop(&mut self) {
        self.close_and_clear();
    }
}

impl<T> ChannelReceiver<T> {
    fn try_recv(&self) -> Option<T> {
        let Ok(mut queue) = self.0.state.lock() else {
            return None;
        };
        queue.items.pop_front()
    }

    fn drain_all(&self) -> (Vec<T>, bool) {
        let Ok(mut queue) = self.0.state.lock() else {
            return (Vec::new(), true);
        };
        let items = queue.items.drain(..).collect();
        let disconnected = queue.senders == 0;
        (items, disconnected)
    }

    fn close(&self) {
        if let Ok(mut queue) = self.0.state.lock() {
            queue.closed = true;
        }
    }

    fn close_and_clear(&self) {
        if let Ok(mut queue) = self.0.state.lock() {
            queue.closed = true;
            queue.items.clear();
        }
    }

    fn is_disconnected(&self) -> bool {
        let Ok(queue) = self.0.state.lock() else {
            return true;
        };
        queue.senders == 0 && queue.items.is_empty()
    }

    #[cfg(feature = "jobs")]
    fn bind_auto_waker(&self, waker: &Waker) {
        self.0.bind_auto_waker(waker);
    }
}

struct DeliverEntityItem<T: Send + Sync + 'static> {
    entity: Entity,
    item: T,
}

impl<T: Send + Sync + 'static> Command for DeliverEntityItem<T> {
    type Out = ();

    fn apply(self, world: &mut World) {
        if world.get_entity(self.entity).is_err() {
            return;
        }
        world.trigger(EntityInboxReceived {
            entity: self.entity,
            item: Some(self.item),
        });
    }
}

fn clear_closed<T: Send + Sync + 'static>(mut world: DeferredWorld, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .try_remove::<InboxClosed<T>>();
}

#[cfg(feature = "jobs")]
fn bind_message_wake<M: Message>(wake: Option<Res<JobWake>>, inbox: Option<Res<MessageInbox<M>>>) {
    let (Some(wake), Some(inbox)) = (wake, inbox) else {
        return;
    };
    inbox.receiver.bind_auto_waker(&wake.0);
}

fn drain_messages<M: Message>(inbox: Option<Res<MessageInbox<M>>>, mut writer: MessageWriter<M>) {
    let Some(inbox) = inbox else {
        return;
    };
    let (items, _) = inbox.receiver.drain_all();
    for item in items {
        writer.write(item);
    }
}

#[cfg(feature = "jobs")]
fn bind_spawn_wake<B: Bundle>(
    wake: Option<Res<JobWake>>,
    resource: Option<Res<SpawnInbox<B>>>,
    components: Query<&EntitySpawnInbox<B>>,
) {
    let Some(wake) = wake else {
        return;
    };
    if let Some(inbox) = resource {
        inbox.receiver.bind_auto_waker(&wake.0);
    }
    for inbox in &components {
        inbox.receiver.bind_auto_waker(&wake.0);
    }
}

fn drain_spawns<B: Bundle>(
    resource: Option<Res<SpawnInbox<B>>>,
    components: Query<&EntitySpawnInbox<B>>,
    mut commands: Commands,
) {
    if let Some(inbox) = resource {
        let (items, _) = inbox.receiver.drain_all();
        for item in items {
            commands.spawn(item);
        }
    }
    for inbox in &components {
        let (items, _) = inbox.receiver.drain_all();
        for item in items {
            commands.spawn(item);
        }
    }
}

#[cfg(feature = "jobs")]
fn bind_entity_wake<T: Send + Sync + 'static>(
    wake: Option<Res<JobWake>>,
    inboxes: Query<&EntityInbox<T>>,
) {
    let Some(wake) = wake else {
        return;
    };
    for inbox in &inboxes {
        inbox.receiver.bind_auto_waker(&wake.0);
    }
}

type EntityInboxes<'w, 's, T> =
    Query<'w, 's, (Entity, &'static EntityInbox<T>, Has<InboxClosed<T>>)>;

fn drain_entities<T: Send + Sync + 'static>(inboxes: EntityInboxes<T>, mut commands: Commands) {
    for (entity, inbox, closed) in &inboxes {
        let (items, disconnected) = inbox.receiver.drain_all();
        for item in items {
            commands.queue(DeliverEntityItem { entity, item });
        }
        if disconnected && !closed {
            commands
                .entity(entity)
                .try_insert(InboxClosed::<T>::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bevy_app::Update;

    use super::*;

    struct CountWake(AtomicUsize);

    impl CountWake {
        fn new() -> Arc<Self> {
            Arc::new(Self(AtomicUsize::new(0)))
        }

        fn count(&self) -> usize {
            self.0.load(Ordering::Acquire)
        }
    }

    impl std::task::Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[derive(Message, Clone, Debug, PartialEq, Eq)]
    struct TestMessage(u32);

    #[derive(Resource, Default)]
    struct ReceivedMessages(Vec<u32>);

    fn record_messages(
        mut reader: MessageReader<TestMessage>,
        mut received: ResMut<ReceivedMessages>,
    ) {
        for message in reader.read() {
            received.0.push(message.0);
        }
    }

    #[derive(Component, Clone, Debug, PartialEq, Eq)]
    struct SpawnedItem(u32);

    #[derive(Resource, Default)]
    struct DeliveredItems(Vec<(Entity, u32)>);

    #[derive(Component)]
    struct DespawnOnItem(u32);

    fn record_entity_item(
        mut trigger: On<EntityInboxReceived<u32>>,
        despawn_on: Query<&DespawnOnItem>,
        mut delivered: ResMut<DeliveredItems>,
        mut commands: Commands,
    ) {
        let entity = trigger.event().entity;
        let Some(value) = trigger.event_mut().take() else {
            return;
        };
        delivered.0.push((entity, value));
        if despawn_on.get(entity).is_ok_and(|rule| rule.0 == value) {
            commands.entity(entity).despawn();
        }
    }

    #[test]
    fn message_inbox_drains_in_fifo_order_and_wakes_on_send() {
        let mut app = App::new();
        let wake = CountWake::new();
        let (bridge, inbox) =
            MessageInbox::<TestMessage>::unbounded_with_waker(Waker::from(Arc::clone(&wake)));
        app.insert_resource(bridge.clone())
            .insert_resource(inbox)
            .init_resource::<ReceivedMessages>()
            .add_plugins(InboxPlugin::<TestMessage>::default())
            .add_systems(Update, record_messages);

        bridge.send(TestMessage(10)).unwrap();
        bridge.send(TestMessage(20)).unwrap();
        bridge.send(TestMessage(30)).unwrap();
        assert_eq!(wake.count(), 3);

        app.update();
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [10, 20, 30]);

        app.update();
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [10, 20, 30]);
    }

    #[test]
    fn bounded_message_inbox_enforces_capacity_and_preserves_order_after_drain() {
        let mut app = App::new();
        app.init_resource::<ReceivedMessages>()
            .add_plugins(InboxPlugin::<TestMessage>::bounded(2))
            .add_systems(Update, record_messages);

        let bridge = app.world().resource::<MessageBridge<TestMessage>>().clone();
        bridge.send(TestMessage(1)).unwrap();
        bridge.send(TestMessage(2)).unwrap();
        let error = bridge.send(TestMessage(3)).unwrap_err();
        assert_eq!(error.kind(), InboxSendErrorKind::Full);
        assert_eq!(error.into_inner(), TestMessage(3));

        app.update();
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [1, 2]);

        bridge.send(TestMessage(3)).unwrap();
        app.update();
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [1, 2, 3]);
    }

    #[cfg(feature = "jobs")]
    #[test]
    fn message_bridge_auto_binds_job_wake_and_wakes_for_queued_items() {
        let mut app = App::new();
        app.init_resource::<ReceivedMessages>()
            .add_plugins(InboxPlugin::<TestMessage>::default())
            .add_systems(Update, record_messages);

        let bridge = app.world().resource::<MessageBridge<TestMessage>>().clone();
        let wake = CountWake::new();
        app.insert_resource(JobWake(Waker::from(Arc::clone(&wake))));

        bridge.send(TestMessage(1)).unwrap();
        assert_eq!(wake.count(), 0);

        app.update();
        assert_eq!(wake.count(), 1);
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [1]);

        bridge.send(TestMessage(2)).unwrap();
        assert_eq!(wake.count(), 2);
        app.update();
        assert_eq!(app.world().resource::<ReceivedMessages>().0, [1, 2]);
    }

    #[test]
    fn spawn_inbox_spawns_resource_and_component_bundles_before_update() {
        let mut app = App::new();
        let wake = CountWake::new();
        app.add_plugins(SpawnInboxPlugin::<SpawnedItem>::bounded(2));

        let resource_bridge = app.world().resource::<SpawnBridge<SpawnedItem>>().clone();
        resource_bridge.bind_waker(Waker::from(Arc::clone(&wake)));
        let (component_bridge, component_inbox) =
            EntitySpawnInbox::<SpawnedItem>::unbounded_with_waker(Waker::from(Arc::clone(&wake)));
        let owner = app.world_mut().spawn(component_inbox).id();

        resource_bridge.send(SpawnedItem(1)).unwrap();
        resource_bridge.send(SpawnedItem(2)).unwrap();
        assert_eq!(
            resource_bridge.send(SpawnedItem(99)).unwrap_err().kind(),
            InboxSendErrorKind::Full
        );
        component_bridge.send(SpawnedItem(3)).unwrap();
        assert_eq!(wake.count(), 3);

        app.update();
        let mut values = app
            .world_mut()
            .query::<&SpawnedItem>()
            .iter(app.world())
            .map(|item| item.0)
            .collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, [1, 2, 3]);

        app.world_mut().entity_mut(owner).despawn();
        assert!(component_bridge.is_closed());
        assert_eq!(
            component_bridge.send(SpawnedItem(4)).unwrap_err().kind(),
            InboxSendErrorKind::Closed
        );
    }

    #[test]
    fn entity_inbox_delivers_per_entity_and_marks_disconnect_after_draining() {
        let mut app = App::new();
        app.init_resource::<DeliveredItems>()
            .add_plugins(EntityInboxPlugin::<u32>::default())
            .add_observer(record_entity_item);

        let wake = CountWake::new();
        let (first_tx, first_rx) =
            EntityInbox::<u32>::unbounded_with_waker(Waker::from(Arc::clone(&wake)));
        let (second_tx, second_rx) = EntityInbox::<u32>::unbounded();
        let first = app.world_mut().spawn(first_rx).id();
        let second = app.world_mut().spawn(second_rx).id();

        first_tx.send(1).unwrap();
        first_tx.send(2).unwrap();
        second_tx.send(10).unwrap();
        drop(first_tx);
        assert_eq!(wake.count(), 3);

        app.update();

        let delivered = &app.world().resource::<DeliveredItems>().0;
        let first_items = delivered
            .iter()
            .filter_map(|(entity, value)| (*entity == first).then_some(*value))
            .collect::<Vec<_>>();
        let second_items = delivered
            .iter()
            .filter_map(|(entity, value)| (*entity == second).then_some(*value))
            .collect::<Vec<_>>();
        assert_eq!(first_items, [1, 2]);
        assert_eq!(second_items, [10]);
        assert!(app.world().get::<InboxClosed<u32>>(first).is_some());
        assert!(app.world().get::<InboxClosed<u32>>(second).is_none());

        let (replacement_tx, replacement_rx) = EntityInbox::<u32>::unbounded();
        app.world_mut().entity_mut(first).insert(replacement_rx);
        app.update();
        assert!(app.world().get::<InboxClosed<u32>>(first).is_none());
        replacement_tx.send(3).unwrap();
        app.update();
        assert!(
            app.world()
                .resource::<DeliveredItems>()
                .0
                .contains(&(first, 3))
        );
    }

    #[test]
    fn despawning_entity_closes_inbox_and_skips_remaining_queued_items() {
        let mut app = App::new();
        app.init_resource::<DeliveredItems>()
            .add_plugins(EntityInboxPlugin::<u32>::default())
            .add_observer(record_entity_item);

        let (tx, rx) = EntityInbox::<u32>::unbounded();
        let entity = app.world_mut().spawn((rx, DespawnOnItem(1))).id();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        app.update();

        assert_eq!(app.world().resource::<DeliveredItems>().0, [(entity, 1)]);
        assert!(app.world().get_entity(entity).is_err());
        assert!(tx.is_closed());
        assert_eq!(tx.send(3).unwrap_err().kind(), InboxSendErrorKind::Closed);
    }
}
