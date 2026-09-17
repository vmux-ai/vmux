use std::collections::BTreeSet;

use crate::event::LayoutOverlayEvent;
use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

pub struct LayoutOverlayPlugin;

impl Plugin for LayoutOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(LayoutOverlayEvent,)>::for_hosts(
            &["layout"],
        ))
        .add_observer(on_layout_overlay_emit);
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutOverlayActive(BTreeSet<String>);

fn on_layout_overlay_emit(
    trigger: On<BinReceive<LayoutOverlayEvent>>,
    mut active: Query<&mut LayoutOverlayActive>,
    mut commands: Commands,
) {
    let event = &trigger.event().payload;
    if event.active {
        if let Ok(mut overlays) = active.get_mut(trigger.event().webview) {
            overlays.0.insert(event.id.clone());
        } else if let Ok(mut webview) = commands.get_entity(trigger.event().webview) {
            webview.insert(LayoutOverlayActive(BTreeSet::from([event.id.clone()])));
        }
        return;
    }

    let Ok(mut overlays) = active.get_mut(trigger.event().webview) else {
        return;
    };
    overlays.0.remove(&event.id);
    if overlays.0.is_empty()
        && let Ok(mut webview) = commands.get_entity(trigger.event().webview)
    {
        webview.remove::<LayoutOverlayActive>();
    }
}
