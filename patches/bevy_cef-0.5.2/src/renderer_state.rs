use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::WebviewRendererTerminatedEvent;

#[derive(Resource, Debug, Deref)]
pub struct WebviewRendererTerminatedSender(pub Sender<WebviewRendererTerminatedEvent>);

#[derive(Resource, Debug)]
pub struct WebviewRendererTerminatedReceiver(pub Receiver<WebviewRendererTerminatedEvent>);

pub(super) struct WebviewRendererStatePlugin;

impl Plugin for WebviewRendererStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(WebviewRendererTerminatedSender(tx))
            .insert_resource(WebviewRendererTerminatedReceiver(rx));
    }
}
