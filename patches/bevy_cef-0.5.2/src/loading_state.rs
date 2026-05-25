use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy_cef_core::prelude::{WebviewCommittedNavigationEvent, WebviewLoadingStateEvent};

#[derive(Resource, Debug, Deref)]
pub struct WebviewLoadingStateSender(pub Sender<WebviewLoadingStateEvent>);

#[derive(Resource, Debug)]
pub struct WebviewLoadingStateReceiver(pub Receiver<WebviewLoadingStateEvent>);

#[derive(Resource, Debug, Deref)]
pub struct WebviewCommittedNavigationSender(pub Sender<WebviewCommittedNavigationEvent>);

#[derive(Resource, Debug)]
pub struct WebviewCommittedNavigationReceiver(pub Receiver<WebviewCommittedNavigationEvent>);

pub(super) struct WebviewLoadingStatePlugin;

impl Plugin for WebviewLoadingStatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(WebviewLoadingStateSender(tx))
            .insert_resource(WebviewLoadingStateReceiver(rx));

        let (nav_tx, nav_rx) = async_channel::unbounded();
        app.insert_resource(WebviewCommittedNavigationSender(nav_tx))
            .insert_resource(WebviewCommittedNavigationReceiver(nav_rx));
    }
}
