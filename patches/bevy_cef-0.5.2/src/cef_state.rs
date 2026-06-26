use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::{MediaPermissionRequest, WebviewCefStateEvent};

#[derive(Resource, Debug, Deref)]
pub struct WebviewCefStateSender(pub Sender<WebviewCefStateEvent>);

#[derive(Resource, Debug)]
pub struct WebviewCefStateReceiver(pub Receiver<WebviewCefStateEvent>);

#[derive(Resource, Debug, Deref)]
pub struct MediaPermissionSender(pub Sender<MediaPermissionRequest>);

#[derive(Resource, Debug)]
pub struct MediaPermissionReceiver(pub Receiver<MediaPermissionRequest>);

pub(super) struct WebviewCefStatePlugin;

impl Plugin for WebviewCefStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        let (media_tx, media_rx) = async_channel::unbounded();
        app.insert_resource(WebviewCefStateSender(tx))
            .insert_resource(WebviewCefStateReceiver(rx))
            .insert_resource(MediaPermissionSender(media_tx))
            .insert_resource(MediaPermissionReceiver(media_rx));
    }
}
