use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::bytecheck::CheckBytes;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use vmux_api::{BinEvent, UiEvent, UiEventPermissions};

#[derive(Resource, Default)]
pub struct BinIpcEventRawBuffer(pub Vec<BinIpcEventRaw>);

#[derive(Resource, Default)]
struct PendingUiInputs {
    queued: Vec<PendingUiInput>,
}

struct PendingUiInput {
    sequence: usize,
    webview: Entity,
    generation: u64,
    revision: u64,
    epoch: Option<u64>,
    allowed_while_blocked: bool,
    dispatch: UiInputDispatch,
}

type UiInputDispatch = Box<dyn FnOnce(&mut World) + Send + Sync>;

#[derive(Resource, Default)]
struct UiEventDecoders(HashMap<&'static str, UiEventDecoder>);

struct UiEventDecoder {
    permission: &'static str,
    allowed_while_blocked: bool,
    decode: fn(&BinIpcEventRaw) -> Option<UiInputDispatch>,
}

impl UiEventDecoders {
    fn register<E>(&mut self)
    where
        E: UiEvent + rkyv::Archive + Send + Sync + 'static,
        E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    {
        self.0.entry(E::id()).or_insert_with(|| UiEventDecoder {
            permission: E::PERMISSION,
            allowed_while_blocked: E::id() == vmux_api::PageReady::id(),
            decode: |event| {
                let payload = decode_ui_event::<E>(event, true)?;
                let webview = event.webview;
                Some(Box::new(move |world| {
                    world.trigger(UiInput { webview, payload });
                }))
            },
        });
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiEventSystems {
    Drain,
    Decode,
    Flush,
}

fn drain_bin_ipc_events(
    receiver: ResMut<BinIpcEventRawReceiver>,
    mut buffer: ResMut<BinIpcEventRawBuffer>,
) {
    buffer.0.clear();
    while let Ok(event) = receiver.0.try_recv() {
        buffer.0.push(event);
    }
}

#[derive(Debug, EntityEvent)]
pub struct UiInput<M: Sync + Send + 'static> {
    #[event_target]
    pub webview: Entity,
    pub payload: M,
}

impl<M> Deref for UiInput<M>
where
    M: Sync + Send + 'static,
{
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

impl<M> DerefMut for UiInput<M>
where
    M: Sync + Send + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.payload
    }
}

pub trait UiEventList {
    fn register_events(app: &mut App);
}

pub struct UiEventPlugin<T> {
    marker: PhantomData<T>,
}

impl<T> Default for UiEventPlugin<T> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<T> Plugin for UiEventPlugin<T>
where
    T: UiEventList + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        app.init_resource::<BinIpcEventRawBuffer>()
            .init_resource::<PendingUiInputs>()
            .init_resource::<UiEventDecoders>();
        T::register_events(app);
    }
}

fn register_event<E>(app: &mut App)
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    app.init_resource::<UiEventDecoders>();
    app.world_mut()
        .resource_mut::<UiEventDecoders>()
        .register::<E>();
}

macro_rules! impl_bin_event_list {
    ($head:ident $(, $tail:ident)*) => {
        impl<$head $(, $tail)*> UiEventList for ($head, $($tail,)*)
        where
            $head: UiEvent + rkyv::Archive + Send + Sync + 'static,
            $head::Archived: rkyv::Deserialize<$head, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
                + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
            $(
                $tail: UiEvent + rkyv::Archive + Send + Sync + 'static,
                $tail::Archived: rkyv::Deserialize<$tail, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
                    + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
            )*
        {
            fn register_events(app: &mut App) {
                register_event::<$head>(app);
                $(
                    register_event::<$tail>(app);
                )*
            }
        }
    };
}

impl_bin_event_list!(T0);
impl_bin_event_list!(T0, T1);
impl_bin_event_list!(T0, T1, T2);
impl_bin_event_list!(T0, T1, T2, T3);
impl_bin_event_list!(T0, T1, T2, T3, T4);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6, T7);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_bin_event_list!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);

fn page_has_permission<'a>(
    permissions: impl Iterator<Item = &'a UiEventPermissions>,
    page_url: &str,
    permission: &str,
) -> bool {
    UiEventPermissions::allows_page(permissions, page_url, permission)
}

fn decode_ui_event<E>(event: &BinIpcEventRaw, permitted: bool) -> Option<E>
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    if event.id != E::id() || !permitted {
        return None;
    }
    rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload).ok()
}

fn navigation_matches(event: &BinIpcEventRaw, current: Option<(u64, u64, Option<u64>)>) -> bool {
    let (Some(generation), Some(revision)) = (event.surface_generation, event.navigation_revision)
    else {
        return false;
    };
    current.is_some_and(|current| {
        current.0 == generation && current.1 == revision && current.2 == event.ipc_epoch
    })
}

fn decode_ui_events(
    buffer: Res<BinIpcEventRawBuffer>,
    decoders: Res<UiEventDecoders>,
    permissions: Query<&UiEventPermissions>,
    browsers: Option<NonSend<Browsers>>,
    blocked: Query<(), With<BinIpcBlocked>>,
    revisions: Query<(
        &BinIpcSurfaceGeneration,
        &BinIpcNavigationRevision,
        Option<&BinIpcEpoch>,
    )>,
    mut pending: ResMut<PendingUiInputs>,
) {
    for (sequence, event) in buffer.0.iter().enumerate() {
        let Some(decoder) = decoders.0.get(event.id.as_str()) else {
            continue;
        };
        if blocked.contains(event.webview) && !decoder.allowed_while_blocked {
            continue;
        }
        let current = browsers
            .as_ref()
            .and_then(|browsers| browsers.bin_ipc_context(&event.webview))
            .map(|(generation, revision, epoch)| (generation, revision, Some(epoch)))
            .or_else(|| {
                revisions
                    .get(event.webview)
                    .ok()
                    .map(|(generation, revision, epoch)| {
                        (generation.0, revision.0, epoch.map(|epoch| epoch.0))
                    })
            });
        if !navigation_matches(event, current) {
            continue;
        }
        let permitted =
            page_has_permission(permissions.iter(), &event.page_url, decoder.permission);
        if permitted && let Some(dispatch) = (decoder.decode)(event) {
            let (Some(generation), Some(revision)) =
                (event.surface_generation, event.navigation_revision)
            else {
                continue;
            };
            let webview = event.webview;
            let epoch = event.ipc_epoch;
            pending.queued.push(PendingUiInput {
                sequence,
                webview,
                generation,
                revision,
                epoch,
                allowed_while_blocked: decoder.allowed_while_blocked,
                dispatch,
            });
        }
    }
}

fn has_pending_ui_inputs(pending: Res<PendingUiInputs>) -> bool {
    !pending.queued.is_empty()
}

fn flush_ui_events(world: &mut World) {
    let mut queued = std::mem::take(&mut world.resource_mut::<PendingUiInputs>().queued);
    queued.sort_by_key(|input| input.sequence);
    for input in queued {
        if !input.allowed_while_blocked && world.get::<BinIpcBlocked>(input.webview).is_some() {
            continue;
        }
        let current = world
            .get_non_send::<Browsers>()
            .and_then(|browsers| browsers.bin_ipc_context(&input.webview))
            .map(|(generation, revision, epoch)| (generation, revision, Some(epoch)))
            .or_else(|| {
                Some((
                    world.get::<BinIpcSurfaceGeneration>(input.webview)?.0,
                    world.get::<BinIpcNavigationRevision>(input.webview)?.0,
                    world.get::<BinIpcEpoch>(input.webview).map(|epoch| epoch.0),
                ))
            });
        if current != Some((input.generation, input.revision, input.epoch)) {
            continue;
        }
        (input.dispatch)(world);
        world.flush();
    }
}

pub(crate) struct BinIpcRawEventPlugin;

impl Plugin for BinIpcRawEventPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(BinIpcEventRawSender(tx))
            .insert_resource(BinIpcEventRawReceiver(rx))
            .init_resource::<BinIpcEventRawBuffer>()
            .init_resource::<PendingUiInputs>()
            .init_resource::<UiEventDecoders>()
            .configure_sets(
                Update,
                (
                    UiEventSystems::Drain,
                    UiEventSystems::Decode,
                    UiEventSystems::Flush,
                )
                    .chain(),
            )
            .add_observer(block_bin_ipc)
            .add_observer(unblock_bin_ipc)
            .add_systems(Update, drain_bin_ipc_events.in_set(UiEventSystems::Drain))
            .add_systems(Update, decode_ui_events.in_set(UiEventSystems::Decode))
            .add_systems(
                Update,
                flush_ui_events
                    .run_if(has_pending_ui_inputs)
                    .in_set(UiEventSystems::Flush),
            );
    }
}

fn block_bin_ipc(trigger: On<Add, BinIpcBlocked>, browsers: Option<NonSend<Browsers>>) {
    if let Some(browsers) = browsers {
        browsers.set_bin_ipc_blocked(&trigger.entity, true);
    }
}

fn unblock_bin_ipc(trigger: On<Remove, BinIpcBlocked>, browsers: Option<NonSend<Browsers>>) {
    if let Some(browsers) = browsers {
        browsers.set_bin_ipc_blocked(&trigger.entity, false);
    }
}

#[derive(Resource)]
pub struct BinIpcEventRawSender(pub Sender<BinIpcEventRaw>);

#[derive(Resource)]
pub(crate) struct BinIpcEventRawReceiver(pub Receiver<BinIpcEventRaw>);

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;

    #[derive(Resource)]
    struct EventTarget(Entity);

    #[derive(Resource, Default)]
    struct Received(Vec<u32>);

    #[derive(Resource, Default)]
    struct ReadyReceived(bool);

    #[derive(Resource, Default)]
    struct ReceivedOrder(Vec<u32>);

    #[vmux_api::ui_event(Eq)]
    struct AlphaEvent {
        value: u32,
    }

    #[vmux_api::ui_event(Eq)]
    struct BetaEvent {
        value: u32,
    }

    #[vmux_api::ui_event(Eq)]
    struct RestrictedEvent {
        value: u32,
    }

    fn despawn_event_target(mut commands: Commands, target: Res<EventTarget>) {
        commands.entity(target.0).despawn();
    }

    fn advance_navigation(mut revisions: Query<&mut BinIpcNavigationRevision>) {
        for mut revision in &mut revisions {
            revision.0 = revision.0.wrapping_add(1);
        }
    }

    fn capture(trigger: On<UiInput<AlphaEvent>>, mut received: ResMut<Received>) {
        received.0.push(trigger.event().payload.value);
    }

    fn capture_ready(_: On<UiInput<vmux_api::PageReady>>, mut received: ResMut<ReadyReceived>) {
        received.0 = true;
    }

    fn capture_alpha_order(trigger: On<UiInput<AlphaEvent>>, mut received: ResMut<ReceivedOrder>) {
        received.0.push(trigger.event().payload.value);
    }

    fn capture_beta_order(trigger: On<UiInput<BetaEvent>>, mut received: ResMut<ReceivedOrder>) {
        received.0.push(trigger.event().payload.value);
    }

    fn capture_and_block(
        trigger: On<UiInput<AlphaEvent>>,
        mut received: ResMut<Received>,
        mut commands: Commands,
    ) {
        received.0.push(trigger.event().payload.value);
        if trigger.event().payload.value == 1 {
            commands
                .entity(trigger.event().webview)
                .insert(BinIpcBlocked);
        }
    }

    fn register_decoder<E>(app: &mut App)
    where
        E: UiEvent + rkyv::Archive + Send + Sync + 'static,
        E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    {
        app.init_resource::<UiEventDecoders>();
        app.world_mut()
            .resource_mut::<UiEventDecoders>()
            .register::<E>();
    }

    #[test]
    fn event_for_a_page_closed_in_the_same_frame_is_ignored() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.insert_resource(EventTarget(target))
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: None,
                id: AlphaEvent::id().to_string(),
                payload: bytes,
            }]))
            .add_systems(
                Update,
                (despawn_event_target, decode_ui_events, flush_ui_events).chain(),
            );
        register_decoder::<AlphaEvent>(&mut app);

        app.update();

        assert!(app.world().get_entity(target).is_err());
    }

    #[test]
    fn matching_document_event_is_delivered() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.init_resource::<Received>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: None,
                id: AlphaEvent::id().to_string(),
                payload: bytes,
            }]))
            .add_observer(capture)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());
        register_decoder::<AlphaEvent>(&mut app);

        app.update();

        assert_eq!(app.world().resource::<Received>().0, vec![7]);
    }

    #[test]
    fn different_event_types_keep_their_arrival_order() {
        let alpha = rkyv::to_bytes::<rkyv::rancor::Error>(&AlphaEvent { value: 1 })
            .expect("serialize")
            .into_vec();
        let beta = rkyv::to_bytes::<rkyv::rancor::Error>(&BetaEvent { value: 2 })
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION, BetaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.init_resource::<ReceivedOrder>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![
                BinIpcEventRaw {
                    webview: target,
                    page_url: "vmux://allowed/".to_string(),
                    surface_generation: Some(3),
                    navigation_revision: Some(7),
                    ipc_epoch: None,
                    id: AlphaEvent::id().to_string(),
                    payload: alpha,
                },
                BinIpcEventRaw {
                    webview: target,
                    page_url: "vmux://allowed/".to_string(),
                    surface_generation: Some(3),
                    navigation_revision: Some(7),
                    ipc_epoch: None,
                    id: BetaEvent::id().to_string(),
                    payload: beta,
                },
            ]))
            .add_observer(capture_alpha_order)
            .add_observer(capture_beta_order)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());
        register_decoder::<AlphaEvent>(&mut app);
        register_decoder::<BetaEvent>(&mut app);

        app.update();

        assert_eq!(app.world().resource::<ReceivedOrder>().0, vec![1, 2]);
    }

    #[test]
    fn duplicate_event_registration_dispatches_once() {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(&AlphaEvent { value: 7 })
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.add_plugins(UiEventPlugin::<(AlphaEvent,)>::default())
            .add_plugins(UiEventPlugin::<(AlphaEvent, BetaEvent)>::default())
            .init_resource::<Received>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: None,
                id: AlphaEvent::id().to_string(),
                payload,
            }]))
            .add_observer(capture)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());

        app.update();

        assert_eq!(app.world().resource::<Received>().0, vec![7]);
    }

    #[test]
    fn earlier_event_commands_invalidate_later_events_in_the_same_batch() {
        let first = rkyv::to_bytes::<rkyv::rancor::Error>(&AlphaEvent { value: 1 })
            .expect("serialize")
            .into_vec();
        let second = rkyv::to_bytes::<rkyv::rancor::Error>(&AlphaEvent { value: 2 })
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.init_resource::<Received>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![
                BinIpcEventRaw {
                    webview: target,
                    page_url: "vmux://allowed/".to_string(),
                    surface_generation: Some(3),
                    navigation_revision: Some(7),
                    ipc_epoch: None,
                    id: AlphaEvent::id().to_string(),
                    payload: first,
                },
                BinIpcEventRaw {
                    webview: target,
                    page_url: "vmux://allowed/".to_string(),
                    surface_generation: Some(3),
                    navigation_revision: Some(7),
                    ipc_epoch: None,
                    id: AlphaEvent::id().to_string(),
                    payload: second,
                },
            ]))
            .add_observer(capture_and_block)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());
        register_decoder::<AlphaEvent>(&mut app);

        app.update();

        assert_eq!(app.world().resource::<Received>().0, vec![1]);
    }

    #[test]
    fn event_is_revalidated_before_deferred_delivery() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
            ))
            .id();
        app.init_resource::<Received>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: None,
                id: AlphaEvent::id().to_string(),
                payload: bytes,
            }]))
            .add_observer(capture)
            .add_systems(
                Update,
                (decode_ui_events, advance_navigation, flush_ui_events).chain(),
            );
        register_decoder::<AlphaEvent>(&mut app);

        app.update();

        assert!(app.world().resource::<Received>().0.is_empty());
    }

    #[test]
    fn page_ready_is_delivered_while_the_surface_is_blocked() {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(&vmux_api::PageReady {})
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[vmux_api::PageReady::PERMISSION],
                },
                BinIpcBlocked,
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
                BinIpcEpoch(2),
            ))
            .id();
        app.init_resource::<ReadyReceived>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: Some(2),
                id: vmux_api::PageReady::id().to_string(),
                payload,
            }]))
            .add_observer(capture_ready)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());
        register_decoder::<vmux_api::PageReady>(&mut app);

        app.update();

        assert!(app.world().resource::<ReadyReceived>().0);
    }

    #[test]
    fn stale_interaction_epoch_is_rejected_after_unblock() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn((
                UiEventPermissions {
                    url: "vmux://allowed/",
                    owns_subtree: false,
                    permissions: &[AlphaEvent::PERMISSION],
                },
                BinIpcSurfaceGeneration(3),
                BinIpcNavigationRevision(7),
                BinIpcEpoch(2),
            ))
            .id();
        app.init_resource::<Received>()
            .init_resource::<PendingUiInputs>()
            .insert_resource(BinIpcEventRawBuffer(vec![BinIpcEventRaw {
                webview: target,
                page_url: "vmux://allowed/".to_string(),
                surface_generation: Some(3),
                navigation_revision: Some(7),
                ipc_epoch: Some(1),
                id: AlphaEvent::id().to_string(),
                payload: bytes,
            }]))
            .add_observer(capture)
            .add_systems(Update, (decode_ui_events, flush_ui_events).chain());
        register_decoder::<AlphaEvent>(&mut app);

        app.update();

        assert!(app.world().resource::<Received>().0.is_empty());
    }

    #[test]
    fn decode_ui_event_ignores_non_matching_id() {
        let payload = BetaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            page_url: String::new(),
            surface_generation: None,
            navigation_revision: None,
            ipc_epoch: None,
            id: BetaEvent::id().to_string(),
            payload: bytes,
        };

        assert!(decode_ui_event::<AlphaEvent>(&raw, true).is_none());
    }

    #[test]
    fn decode_ui_event_decodes_matching_id() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            page_url: String::new(),
            surface_generation: None,
            navigation_revision: None,
            ipc_epoch: None,
            id: AlphaEvent::id().to_string(),
            payload: bytes,
        };

        let decoded = decode_ui_event::<AlphaEvent>(&raw, true).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_ui_event_rejects_an_unexpected_page_host() {
        let payload = RestrictedEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            page_url: "vmux://other/".to_string(),
            surface_generation: None,
            navigation_revision: None,
            ipc_epoch: None,
            id: RestrictedEvent::id().to_string(),
            payload: bytes,
        };

        assert!(decode_ui_event::<RestrictedEvent>(&raw, false).is_none());
    }

    #[test]
    fn navigation_revision_rejects_buffered_events_from_an_old_page() {
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            page_url: "vmux://allowed/".to_string(),
            surface_generation: Some(3),
            navigation_revision: Some(7),
            ipc_epoch: Some(4),
            id: AlphaEvent::id().to_string(),
            payload: Vec::new(),
        };

        assert!(navigation_matches(&raw, Some((3, 7, Some(4)))));
        assert!(!navigation_matches(&raw, Some((3, 7, Some(5)))));
        assert!(!navigation_matches(&raw, Some((3, 8, Some(4)))));
        assert!(!navigation_matches(&raw, Some((4, 7, Some(4)))));
        assert!(!navigation_matches(&raw, None));
    }

    #[test]
    fn page_manifest_permissions_restrict_event_types() {
        let permissions = [UiEventPermissions {
            url: "vmux://allowed/",
            owns_subtree: false,
            permissions: &[RestrictedEvent::PERMISSION],
        }];

        assert!(page_has_permission(
            permissions.iter(),
            "vmux://allowed/",
            RestrictedEvent::PERMISSION
        ));
        assert!(!page_has_permission(
            permissions.iter(),
            "vmux://other/",
            RestrictedEvent::PERMISSION
        ));
        assert!(!page_has_permission(
            permissions.iter(),
            "vmux://allowed/",
            AlphaEvent::PERMISSION
        ));
    }
}
