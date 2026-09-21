use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::bytecheck::CheckBytes;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use vmux_api::{BinEventTarget, UiEvent};

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
pub struct BinReceive<M: Sync + Send + 'static> {
    #[event_target]
    pub webview: Entity,
    pub payload: M,
}

impl<M> Deref for BinReceive<M>
where
    M: Sync + Send + 'static,
{
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

impl<M> DerefMut for BinReceive<M>
where
    M: Sync + Send + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.payload
    }
}

pub trait BinEventList {
    fn register_events(app: &mut App);
}

pub struct BinEventEmitterPlugin<T> {
    marker: PhantomData<T>,
}

impl<T> Default for BinEventEmitterPlugin<T> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<T> Plugin for BinEventEmitterPlugin<T>
where
    T: BinEventList + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        T::register_events(app);
    }
}

fn host_allowed(target: BinEventTarget, host: &str) -> bool {
    target.accepts(host)
}

fn register_event<E>(app: &mut App)
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    app.add_systems(
        Update,
        (move |commands: Commands, buffer: Res<BinIpcEventRawBuffer>| {
            receive_bin_events::<E>(commands, buffer);
        })
        .after(drain_bin_ipc_events),
    );
}

macro_rules! impl_bin_event_list {
    ($head:ident $(, $tail:ident)*) => {
        impl<$head $(, $tail)*> BinEventList for ($head, $($tail,)*)
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

fn decode_bin_event<E>(event: &BinIpcEventRaw) -> Option<E>
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    if event.id != E::id() || !host_allowed(E::TARGET, &event.host) {
        return None;
    }
    rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload).ok()
}

fn receive_bin_events<E>(mut commands: Commands, buffer: Res<BinIpcEventRawBuffer>)
where
    E: UiEvent + rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        if let Some(payload) = decode_bin_event::<E>(event) {
            commands.trigger(BinReceive {
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
/// engines land in one `BinReceive` path rather than each growing a routing layer.
#[derive(Resource)]
pub struct BinIpcEventRawSender(pub Sender<BinIpcEventRaw>);

#[derive(Resource)]
pub(crate) struct BinIpcEventRawReceiver(pub Receiver<BinIpcEventRaw>);

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;

    #[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    #[vmux_api::ui_event(name = "alpha", target = any)]
    struct AlphaEvent {
        value: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    #[vmux_api::ui_event(name = "beta", target = any)]
    struct BetaEvent {
        value: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    #[vmux_api::ui_event(name = "restricted", target = "allowed")]
    struct RestrictedEvent {
        value: u32,
    }

    #[test]
    fn decode_bin_event_ignores_non_matching_id() {
        let payload = BetaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            host: String::new(),
            id: BetaEvent::id().to_string(),
            payload: bytes,
        };

        assert!(decode_bin_event::<AlphaEvent>(&raw).is_none());
    }

    #[test]
    fn decode_bin_event_decodes_matching_id() {
        let payload = AlphaEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            host: String::new(),
            id: AlphaEvent::id().to_string(),
            payload: bytes,
        };

        let decoded = decode_bin_event::<AlphaEvent>(&raw).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_bin_event_rejects_an_unexpected_page_host() {
        let payload = RestrictedEvent { value: 7 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&payload)
            .expect("serialize")
            .into_vec();
        let raw = BinIpcEventRaw {
            webview: Entity::PLACEHOLDER,
            host: "other".to_string(),
            id: RestrictedEvent::id().to_string(),
            payload: bytes,
        };

        assert!(decode_bin_event::<RestrictedEvent>(&raw).is_none());
    }

    #[test]
    fn host_allowed_without_owner_accepts_any_host() {
        assert!(host_allowed(BinEventTarget::Any, "history"));
        assert!(host_allowed(BinEventTarget::Any, ""));
    }

    #[test]
    fn host_allowed_restricts_to_owner_hosts() {
        assert!(host_allowed(BinEventTarget::Host("history"), "history"));
        assert!(!host_allowed(
            BinEventTarget::Host("history"),
            "command-bar"
        ));
        assert!(host_allowed(
            BinEventTarget::Hosts(&["debug", "layout"]),
            "layout"
        ));
        assert!(host_allowed(
            BinEventTarget::Hosts(&["debug", "layout"]),
            "debug"
        ));
        assert!(!host_allowed(
            BinEventTarget::Hosts(&["debug", "layout"]),
            "terminal"
        ));
        assert!(!host_allowed(BinEventTarget::Hosts(&[]), "history"));
    }
}
