use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::message::{MessageCursor, Messages};
use bevy_ecs::prelude::{
    Commands, Component, Entity, EntityEvent, In, IntoScheduleConfigs, Message, Query,
    ReflectComponent, Resource, SystemSet, With, World,
};
use bevy_ecs::system::SystemId;
use bevy_reflect::Reflect;
use rkyv::api::high::HighSerializer;
use rkyv::bytecheck::CheckBytes;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{BinEvent, HostEvent, PageReady, UiEvent, UiEventPermissions};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiIpcSet {
    Sync,
    Dispatch,
}

#[derive(Resource, Default)]
struct UiReceiveCursor(MessageCursor<UiReceiveRaw>);

#[derive(Resource, Default)]
struct UiEventDispatchers(HashMap<&'static str, SystemId<In<UiReceiveRaw>, ()>>);

pub struct UiIpcPlugin;

impl Plugin for UiIpcPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<UiEmitEvent>()
            .init_resource::<UiReceiveCursor>()
            .init_resource::<UiEventDispatchers>()
            .add_message::<UiReceiveRaw>()
            .configure_sets(Update, (UiIpcSet::Sync, UiIpcSet::Dispatch).chain())
            .add_systems(Update, dispatch.in_set(UiIpcSet::Dispatch));
    }
}

pub struct UiIpcEventPlugin<E>(PhantomData<fn() -> E>);

impl<E> Default for UiIpcEventPlugin<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<E> Plugin for UiIpcEventPlugin<E>
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<UiIpcPlugin>() {
            app.add_plugins(UiIpcPlugin);
        }
        let existing = app
            .world()
            .resource::<UiEventDispatchers>()
            .0
            .contains_key(E::id());
        if !existing {
            let system = app.register_system(deliver::<E>);
            app.world_mut()
                .resource_mut::<UiEventDispatchers>()
                .0
                .insert(E::id(), system);
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct UiIpcGeneration(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct UiIpcNavigationRevision(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct UiIpcEpoch(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct UiIpcBlocked;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct UiIpcReady;

#[derive(Clone, Debug, Default)]
pub struct UiEmitReceipt(Arc<AtomicBool>);

impl UiEmitReceipt {
    pub fn delivered(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn mark_delivered(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Reflect, Debug, Clone, EntityEvent)]
#[reflect(opaque)]
pub struct UiEmitEvent {
    #[event_target]
    pub target: Entity,
    pub id: &'static str,
    pub permission: &'static str,
    pub payload: Arc<[u8]>,
    pub generation: u64,
    pub navigation_revision: u64,
    pub ipc_epoch: u64,
    #[reflect(ignore)]
    pub receipt: UiEmitReceipt,
}

impl UiEmitEvent {
    pub fn encode<C, P>(target: Entity, authority: (u64, u64, u64), value: &P) -> Self
    where
        C: HostEvent,
        P: for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .unwrap_or_else(|error| panic!("failed to serialize host event {}: {error:?}", C::id()))
            .into_vec();
        let (generation, navigation_revision, ipc_epoch) = authority;
        Self {
            target,
            id: C::id(),
            permission: C::PERMISSION,
            payload: payload.into(),
            generation,
            navigation_revision,
            ipc_epoch,
            receipt: UiEmitReceipt::default(),
        }
    }

    pub const fn matches_authority(
        &self,
        generation: u64,
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> bool {
        self.generation == generation
            && self.navigation_revision == navigation_revision
            && self.ipc_epoch == ipc_epoch
            && generation != 0
            && navigation_revision != 0
            && ipc_epoch != 0
    }
}

#[derive(Clone, Copy, Debug, EntityEvent)]
pub struct UiPageReady {
    #[event_target]
    pub page: Entity,
}

#[derive(Clone, Copy, Debug, EntityEvent)]
pub struct UiPageActivated {
    #[event_target]
    pub page: Entity,
}

#[derive(Message, Clone, Debug)]
pub struct UiReceiveRaw {
    pub page: Entity,
    pub id: String,
    pub payload: Vec<u8>,
    pub page_url: String,
    pub generation: u64,
    pub navigation_revision: u64,
    pub ipc_epoch: u64,
}

impl UiReceiveRaw {
    pub fn from_event<E>(
        page: Entity,
        page_url: impl Into<String>,
        authority: (u64, u64, u64),
        value: &E,
    ) -> Self
    where
        E: UiEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .unwrap_or_else(|error| panic!("failed to serialize UI event {}: {error:?}", E::id()))
            .into_vec();
        let (generation, navigation_revision, ipc_epoch) = authority;
        Self {
            page,
            id: E::id().to_string(),
            payload,
            page_url: page_url.into(),
            generation,
            navigation_revision,
            ipc_epoch,
        }
    }

    fn matches_authority(&self, generation: u64, revision: u64, epoch: u64) -> bool {
        generation != 0
            && revision != 0
            && epoch != 0
            && self.generation == generation
            && self.navigation_revision == revision
            && self.ipc_epoch == epoch
    }
}

#[derive(Debug, EntityEvent)]
pub struct UiIpcInput<M: Send + Sync + 'static> {
    #[event_target]
    pub page: Entity,
    pub payload: M,
}

impl<M: Send + Sync + 'static> Deref for UiIpcInput<M> {
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

impl<M: Send + Sync + 'static> DerefMut for UiIpcInput<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.payload
    }
}

fn dispatch(world: &mut World) {
    let mut cursor = world
        .remove_resource::<UiReceiveCursor>()
        .unwrap_or_default();
    let batch: Vec<UiReceiveRaw> = world
        .get_resource::<Messages<UiReceiveRaw>>()
        .map(|messages| cursor.0.read(messages).cloned().collect())
        .unwrap_or_default();
    world.insert_resource(cursor);

    for event in batch {
        if event.id == PageReady::id() {
            let authority = world.get_entity(event.page).ok().and_then(|entity| {
                Some((
                    entity.get::<UiIpcGeneration>()?.0,
                    entity.get::<UiIpcNavigationRevision>()?.0,
                    entity.get::<UiIpcEpoch>()?.0,
                ))
            });
            let allowed = authority.is_some_and(|(generation, revision, epoch)| {
                event.matches_authority(generation, revision, epoch)
            }) && {
                let mut query = world.query::<&UiEventPermissions>();
                UiEventPermissions::allows_page(
                    query.iter(world),
                    &event.page_url,
                    PageReady::PERMISSION,
                )
            };
            if allowed && rkyv::from_bytes::<PageReady, rkyv::rancor::Error>(&event.payload).is_ok()
            {
                world.trigger(UiPageReady { page: event.page });
                world.flush();
            }
            continue;
        }
        let system = world
            .resource::<UiEventDispatchers>()
            .0
            .get(event.id.as_str())
            .copied();
        if let Some(system) = system {
            let _ = world.run_system_with(system, event);
            world.flush();
        }
    }
}

fn deliver<E>(
    In(event): In<UiReceiveRaw>,
    blocked: Query<(), With<UiIpcBlocked>>,
    authority: Query<(&UiIpcGeneration, &UiIpcNavigationRevision, &UiIpcEpoch)>,
    permissions: Query<&UiEventPermissions>,
    mut commands: Commands,
) where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    if event.id != PageReady::id() && blocked.contains(event.page) {
        return;
    }
    let Ok((generation, revision, epoch)) = authority.get(event.page) else {
        return;
    };
    if !event.matches_authority(generation.0, revision.0, epoch.0) {
        return;
    }
    if !UiEventPermissions::allows_page(permissions.iter(), &event.page_url, E::PERMISSION) {
        return;
    }
    let Ok(payload) = rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload) else {
        return;
    };
    commands.trigger(UiIpcInput {
        page: event.page,
        payload,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::prelude::{On, ResMut, Resource};

    #[vmux_api::host_event]
    struct HostSample {
        value: u32,
    }

    #[vmux_api::ui_event(Eq)]
    struct IncrementEvent {
        delta: i32,
    }

    #[vmux_api::ui_event(Eq)]
    struct LabelEvent {
        text: String,
    }

    #[derive(Resource, Default)]
    struct Received(Vec<i32>);

    #[derive(Resource, Default)]
    struct OrderedLog(Vec<String>);

    #[derive(Resource, Default)]
    struct ReadyCount(usize);

    fn on_increment(trigger: On<UiIpcInput<IncrementEvent>>, mut received: ResMut<Received>) {
        received.0.push(trigger.event().delta);
    }

    fn on_increment_log(trigger: On<UiIpcInput<IncrementEvent>>, mut log: ResMut<OrderedLog>) {
        log.0.push(format!("inc:{}", trigger.event().delta));
    }

    fn on_label_log(trigger: On<UiIpcInput<LabelEvent>>, mut log: ResMut<OrderedLog>) {
        log.0.push(format!("label:{}", trigger.event().text));
    }

    fn on_increment_block_page(
        trigger: On<UiIpcInput<IncrementEvent>>,
        mut received: ResMut<Received>,
        mut commands: Commands,
    ) {
        received.0.push(trigger.event().delta);
        commands.entity(trigger.event().page).insert(UiIpcBlocked);
    }

    fn on_ready(_: On<UiPageReady>, mut count: ResMut<ReadyCount>) {
        count.0 += 1;
    }

    #[test]
    fn ui_emit_event_round_trips_and_tracks_receipt() {
        let original = HostSample { value: 42 };
        let event = UiEmitEvent::encode::<HostSample, _>(Entity::PLACEHOLDER, (3, 5, 7), &original);
        let receipt = event.receipt.clone();
        assert!(!receipt.delivered());
        assert_eq!(event.id, HostSample::id());
        assert_eq!(event.permission, HostSample::PERMISSION);
        assert!(event.matches_authority(3, 5, 7));
        assert!(!event.matches_authority(3, 6, 7));
        assert!(!event.matches_authority(0, 5, 7));
        event.receipt.mark_delivered();
        assert!(receipt.delivered());
        let decoded =
            rkyv::from_bytes::<HostSample, rkyv::rancor::Error>(&event.payload).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn ui_ipc_event_plugin_enforces_authority_blocking_and_permissions() {
        let mut app = App::new();
        app.add_plugins(UiIpcEventPlugin::<IncrementEvent>::default())
            .init_resource::<Received>()
            .init_resource::<ReadyCount>()
            .add_observer(on_increment)
            .add_observer(on_ready);
        let page = app
            .world_mut()
            .spawn((
                UiIpcGeneration(2),
                UiIpcNavigationRevision(4),
                UiIpcEpoch(1),
            ))
            .id();
        app.world_mut().spawn(UiEventPermissions {
            url: "app://counter",
            owns_subtree: false,
            permissions: &[IncrementEvent::PERMISSION, PageReady::PERMISSION],
        });

        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://counter",
            (2, 4, 1),
            &IncrementEvent { delta: 5 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://counter",
            (2, 3, 1),
            &IncrementEvent { delta: 99 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://other",
            (2, 4, 1),
            &IncrementEvent { delta: 88 },
        ));
        app.update();
        assert_eq!(app.world().resource::<Received>().0, vec![5]);

        app.world_mut().entity_mut(page).insert(UiIpcBlocked);
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://counter",
            (2, 4, 1),
            &IncrementEvent { delta: 7 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://counter",
            (2, 4, 1),
            &PageReady {},
        ));
        app.update();
        assert_eq!(app.world().resource::<Received>().0, vec![5]);
        assert_eq!(app.world().resource::<ReadyCount>().0, 1);
    }

    #[test]
    fn interleaved_event_types_keep_arrival_order() {
        let mut app = App::new();
        app.add_plugins((
            UiIpcEventPlugin::<IncrementEvent>::default(),
            UiIpcEventPlugin::<LabelEvent>::default(),
        ))
        .init_resource::<OrderedLog>()
        .add_observer(on_increment_log)
        .add_observer(on_label_log);
        let page = app
            .world_mut()
            .spawn((
                UiIpcGeneration(1),
                UiIpcNavigationRevision(1),
                UiIpcEpoch(1),
            ))
            .id();
        app.world_mut().spawn(UiEventPermissions {
            url: "app://ordered",
            owns_subtree: false,
            permissions: &[IncrementEvent::PERMISSION, LabelEvent::PERMISSION],
        });

        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://ordered",
            (1, 1, 1),
            &IncrementEvent { delta: 1 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://ordered",
            (1, 1, 1),
            &LabelEvent { text: "mid".into() },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://ordered",
            (1, 1, 1),
            &IncrementEvent { delta: 2 },
        ));
        app.update();
        assert_eq!(
            app.world().resource::<OrderedLog>().0,
            vec!["inc:1", "label:mid", "inc:2"]
        );
    }

    #[test]
    fn earlier_observer_invalidation_drops_later_events_in_same_batch() {
        let mut app = App::new();
        app.add_plugins(UiIpcEventPlugin::<IncrementEvent>::default())
            .init_resource::<Received>()
            .init_resource::<ReadyCount>()
            .add_observer(on_increment_block_page)
            .add_observer(on_ready);
        let page = app
            .world_mut()
            .spawn((
                UiIpcGeneration(1),
                UiIpcNavigationRevision(1),
                UiIpcEpoch(1),
            ))
            .id();
        app.world_mut().spawn(UiEventPermissions {
            url: "app://batch",
            owns_subtree: false,
            permissions: &[IncrementEvent::PERMISSION, PageReady::PERMISSION],
        });

        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://batch",
            (1, 1, 1),
            &IncrementEvent { delta: 10 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://batch",
            (1, 1, 1),
            &IncrementEvent { delta: 20 },
        ));
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://batch",
            (1, 1, 1),
            &PageReady {},
        ));
        app.update();
        assert_eq!(app.world().resource::<Received>().0, vec![10]);
        assert_eq!(app.world().resource::<ReadyCount>().0, 1);
    }
}
