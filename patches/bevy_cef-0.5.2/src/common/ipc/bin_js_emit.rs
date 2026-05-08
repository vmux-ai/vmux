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

pub struct BinJsEmitEventPlugin<E>(PhantomData<E>);

impl<E> Plugin for BinJsEmitEventPlugin<E>
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    fn build(&self, app: &mut App) {
        app.add_systems(Update, receive_bin_events::<E>.after(drain_bin_ipc_events));
    }
}

impl<E> Default for BinJsEmitEventPlugin<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

fn receive_bin_events<E>(mut commands: Commands, buffer: Res<BinIpcEventRawBuffer>)
where
    E: rkyv::Archive + Send + Sync + 'static,
    E::Archived: rkyv::Deserialize<E, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    for event in &buffer.0 {
        if let Ok(payload) = rkyv::from_bytes::<E, rkyv::rancor::Error>(&event.payload) {
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
