use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

use crate::event::CommandBarPanelActiveEvent;

pub struct CommandBarPanelPlugin;

impl Plugin for CommandBarPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            BinEventEmitterPlugin::<(CommandBarPanelActiveEvent,)>::for_hosts(&["layout"]),
        )
        .add_observer(on_command_bar_panel_active);
    }
}

/// The command bar panel in the layout page holds a focused DOM field.
///
/// Sits on the `LayoutCef` webview entity, exactly like [`crate::bookmark::BookmarkTextInputActive`],
/// and feeds the same "the layout page owns the keyboard" rule: `CefKeyboardTarget` moves to the
/// layout shell, OSR focus follows it, and AppKit first responder stays with winit so keys route
/// winit -> Bevy -> `send_key_event` -> the focused element.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBarPanelActive;

fn on_command_bar_panel_active(
    trigger: On<BinReceive<CommandBarPanelActiveEvent>>,
    mut commands: Commands,
) {
    let Ok(mut webview) = commands.get_entity(trigger.event().webview) else {
        return;
    };
    if trigger.event().payload.active {
        webview.insert(CommandBarPanelActive);
    } else {
        webview.remove::<CommandBarPanelActive>();
    }
}

#[cfg(test)]
#[path = "panel.test.rs"]
mod tests;
