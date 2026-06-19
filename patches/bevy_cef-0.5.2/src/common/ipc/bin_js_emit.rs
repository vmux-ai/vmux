use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::bytecheck::CheckBytes;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

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
    fn register_events(
        app: &mut App,
        id_override: Option<&'static str>,
        owner_hosts: Option<&'static [&'static str]>,
    );
}

pub struct BinEventEmitterPlugin<T> {
    id: Option<&'static str>,
    owner_hosts: Option<&'static [&'static str]>,
    marker: PhantomData<T>,
}

impl<E> BinEventEmitterPlugin<(E,)> {
    pub const fn with_id(id: &'static str) -> Self {
        Self {
            id: Some(id),
            owner_hosts: None,
            marker: PhantomData,
        }
    }
}

impl<T> BinEventEmitterPlugin<T> {
    pub const fn for_hosts(owner_hosts: &'static [&'static str]) -> Self {
        Self {
            id: None,
            owner_hosts: Some(owner_hosts),
            marker: PhantomData,
        }
    }
}

impl<T> Default for BinEventEmitterPlugin<T> {
    fn default() -> Self {
        Self {
            id: None,
            owner_hosts: None,
            marker: PhantomData,
        }
    }
}

impl<T> Plugin for BinEventEmitterPlugin<T>
where
    T: BinEventList + Send + Sync + 'static,
{
    fn build(&self, app: &mut App) {
        T::register_events(app, self.id, self.owner_hosts);
    }
}

fn host_allowed(owner_hosts: Option<&[&str]>, host: &str) -> bool {
    match owner_hosts {
        None => true,
        Some(list) => list.contains(&host),
    }
}

fn register_event<E>(app: &mut App, id: &'static str, owner_hosts: Option<&'static [&'static str]>)
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    app.add_systems(
        Update,
        (move |commands: Commands, buffer: Res<BinIpcEventRawBuffer>| {
            receive_bin_events::<E>(commands, buffer, id, owner_hosts);
        })
        .after(drain_bin_ipc_events),
    );
}

macro_rules! impl_bin_event_list {
    ($head:ident $(, $tail:ident)*) => {
        impl<$head $(, $tail)*> BinEventList for ($head, $($tail,)*)
        where
            $head: rkyv::Archive + Send + Sync + 'static,
            $head::Archived: rkyv::Deserialize<$head, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
                + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
            $(
                $tail: rkyv::Archive + Send + Sync + 'static,
                $tail::Archived: rkyv::Deserialize<$tail, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
                    + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
            )*
        {
            fn register_events(
                app: &mut App,
                id_override: Option<&'static str>,
                owner_hosts: Option<&'static [&'static str]>,
            ) {
                let head_id = id_override.unwrap_or_else(bin_ipc_event_id::<$head>);
                register_event::<$head>(app, head_id, owner_hosts);
                $(
                    register_event::<$tail>(app, bin_ipc_event_id::<$tail>(), owner_hosts);
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

fn bin_ipc_event_id<E>() -> &'static str {
    std::any::type_name::<E>()
}

fn decode_bin_event<E>(event: &BinIpcEventRaw, id: &str) -> Option<E>
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    if event.id != id {
        return None;
    }
    rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload).ok()
}

fn receive_bin_events<E>(
    mut commands: Commands,
    buffer: Res<BinIpcEventRawBuffer>,
    id: &str,
    owner_hosts: Option<&'static [&'static str]>,
) where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        if let Some(payload) = decode_bin_event::<E>(event, id) {
            if !host_allowed(owner_hosts, &event.host) {
                webview_debug_log(format!(
                    "ipc: dropped '{}' from host '{}' (owner {:?})",
                    id, event.host, owner_hosts
                ));
                continue;
            }
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

#[derive(Resource)]
pub(crate) struct BinIpcEventRawSender(pub Sender<BinIpcEventRaw>);

#[derive(Resource)]
pub(crate) struct BinIpcEventRawReceiver(pub Receiver<BinIpcEventRaw>);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct AlphaEvent {
        value: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct BetaEvent {
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
            id: bin_ipc_event_id::<BetaEvent>().to_string(),
            payload: bytes,
        };

        assert!(decode_bin_event::<AlphaEvent>(&raw, bin_ipc_event_id::<AlphaEvent>()).is_none());
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
            id: bin_ipc_event_id::<AlphaEvent>().to_string(),
            payload: bytes,
        };

        let decoded =
            decode_bin_event::<AlphaEvent>(&raw, bin_ipc_event_id::<AlphaEvent>()).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn host_allowed_without_owner_accepts_any_host() {
        assert!(host_allowed(None, "history"));
        assert!(host_allowed(None, ""));
    }

    #[test]
    fn host_allowed_restricts_to_owner_hosts() {
        assert!(host_allowed(Some(&["history"]), "history"));
        assert!(!host_allowed(Some(&["history"]), "command-bar"));
        assert!(host_allowed(Some(&["debug", "layout"]), "layout"));
        assert!(host_allowed(Some(&["debug", "layout"]), "debug"));
        assert!(!host_allowed(Some(&["debug", "layout"]), "terminal"));
        assert!(!host_allowed(Some(&[]), "history"));
    }
}
