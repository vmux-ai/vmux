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

pub struct BinJsEmitEventPlugin<E> {
    id: &'static str,
    marker: PhantomData<E>,
}

impl<E> BinJsEmitEventPlugin<E> {
    pub const fn with_id(id: &'static str) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
}

impl<E> Plugin for BinJsEmitEventPlugin<E>
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        let id = self.id;
        app.add_systems(
            Update,
            (move |commands: Commands, buffer: Res<BinIpcEventRawBuffer>| {
                receive_bin_events::<E>(commands, buffer, id);
            })
            .after(drain_bin_ipc_events),
        );
    }
}

impl<E> Default for BinJsEmitEventPlugin<E> {
    fn default() -> Self {
        Self::with_id(bin_ipc_event_id::<E>())
    }
}

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

fn receive_bin_events<E>(mut commands: Commands, buffer: Res<BinIpcEventRawBuffer>, id: &str)
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        if let Some(payload) = decode_bin_event::<E>(event, id) {
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
            id: bin_ipc_event_id::<AlphaEvent>().to_string(),
            payload: bytes,
        };

        let decoded =
            decode_bin_event::<AlphaEvent>(&raw, bin_ipc_event_id::<AlphaEvent>()).unwrap();

        assert_eq!(decoded, payload);
    }
}
