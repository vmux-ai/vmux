use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::bytecheck::CheckBytes;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use vmux_api::{UiEvent, UiEventPermissions};

#[derive(Resource, Default)]
pub struct BinIpcEventRawBuffer(pub Vec<BinIpcEventRaw>);

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
        T::register_events(app);
    }
}

fn register_event<E>(app: &mut App)
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    app.add_systems(
        Update,
        (move |commands: Commands,
               buffer: Res<BinIpcEventRawBuffer>,
               permissions: Query<&UiEventPermissions>| {
            receive_ui_events::<E>(commands, buffer, permissions);
        })
        .after(drain_bin_ipc_events),
    );
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

fn receive_ui_events<E>(
    mut commands: Commands,
    buffer: Res<BinIpcEventRawBuffer>,
    permissions: Query<&UiEventPermissions>,
) where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        let permitted = page_has_permission(permissions.iter(), &event.page_url, E::PERMISSION);
        if let Some(payload) = decode_ui_event::<E>(event, permitted) {
            commands.trigger(UiInput {
                webview: event.webview,
                payload,
            });
        }
    }
}

pub(crate) struct BinIpcRawEventPlugin;

impl Plugin for BinIpcRawEventPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(BinIpcEventRawSender(tx))
            .insert_resource(BinIpcEventRawReceiver(rx))
            .init_resource::<BinIpcEventRawBuffer>()
            .add_systems(Update, drain_bin_ipc_events);
    }
}

/// Public because CEF's client handler is no longer the only producer: the wry-hosted layout
/// decodes the same envelope out of a string IPC body and pushes onto this channel, so that both
/// engines land in one `UiInput` path rather than each growing a routing layer.
#[derive(Resource)]
pub struct BinIpcEventRawSender(pub Sender<BinIpcEventRaw>);

#[derive(Resource)]
pub(crate) struct BinIpcEventRawReceiver(pub Receiver<BinIpcEventRaw>);

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;

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

    #[test]
    fn decode_ui_event_ignores_non_matching_id() {
        let payload = BetaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            page_url: String::new(),
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
            id: RestrictedEvent::id().to_string(),
            payload: bytes,
        };

        assert!(decode_ui_event::<RestrictedEvent>(&raw, false).is_none());
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
