use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::HistorySwipeRaw;

#[derive(Resource)]
pub struct HistorySwipeEventSender(pub Sender<HistorySwipeRaw>);

#[derive(Resource)]
pub struct HistorySwipeEventReceiver(pub Receiver<HistorySwipeRaw>);

pub(super) struct HistorySwipeIpcPlugin;

impl Plugin for HistorySwipeIpcPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = async_channel::bounded(32);
        app.insert_resource(HistorySwipeEventSender(sender))
            .insert_resource(HistorySwipeEventReceiver(receiver));
    }
}
