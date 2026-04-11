use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::WebviewChromeStateEvent;

#[derive(Resource, Debug, Deref)]
pub struct WebviewChromeStateSender(pub Sender<WebviewChromeStateEvent>);

#[derive(Resource, Debug)]
pub struct WebviewChromeStateReceiver(pub Receiver<WebviewChromeStateEvent>);

pub(super) struct WebviewChromeStatePlugin;

impl Plugin for WebviewChromeStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(WebviewChromeStateSender(tx))
            .insert_resource(WebviewChromeStateReceiver(rx));
    }
}
