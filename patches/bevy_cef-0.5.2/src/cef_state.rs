use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::WebviewCefStateEvent;

#[derive(Resource, Debug, Deref)]
pub struct WebviewCefStateSender(pub Sender<WebviewCefStateEvent>);

#[derive(Resource, Debug)]
pub struct WebviewCefStateReceiver(pub Receiver<WebviewCefStateEvent>);

pub(super) struct WebviewCefStatePlugin;

impl Plugin for WebviewCefStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(WebviewCefStateSender(tx))
            .insert_resource(WebviewCefStateReceiver(rx));
    }
}
