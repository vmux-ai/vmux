use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::WebviewLoadingStateEvent;

/// Sender installed by [`WebviewLoadingStatePlugin`]; passed into [`bevy_cef_core::prelude::Browsers::create_browser`].
#[derive(Resource, Debug, Deref)]
pub struct WebviewLoadingStateSender(pub Sender<WebviewLoadingStateEvent>);

/// Drain in your app to react to CEF [`CefLoadHandler::on_loading_state_change`](https://cef-builds.spotifycdn.com/docs/145.0/classCefLoadHandler.html).
#[derive(Resource, Debug)]
pub struct WebviewLoadingStateReceiver(pub Receiver<WebviewLoadingStateEvent>);

pub(super) struct WebviewLoadingStatePlugin;

impl Plugin for WebviewLoadingStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(WebviewLoadingStateSender(tx))
            .insert_resource(WebviewLoadingStateReceiver(rx));
    }
}
