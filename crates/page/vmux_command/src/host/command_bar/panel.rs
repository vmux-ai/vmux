use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

use crate::CommandBar;
use crate::event::CommandBarPanelActiveEvent;
use vmux_core::overlay::OverlayShownInline;

pub struct CommandBarPanelPlugin;

impl Plugin for CommandBarPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            BinEventEmitterPlugin::<(CommandBarPanelActiveEvent,)>::for_hosts(&["layout"]),
        )
        .add_observer(on_command_bar_panel_active)
        .add_systems(Update, mark_command_bar_shown_inline);
    }
}

/// The command bar panel in the layout page holds a focused DOM field.
///
/// Sits on the webview that renders the panel, exactly like the bookmark field's own active marker,
/// and feeds the same "the layout page owns the keyboard" rule: `CefKeyboardTarget` moves to the
/// layout shell, OSR focus follows it, and AppKit first responder stays with winit so keys route
/// winit -> Bevy -> `send_key_event` -> the focused element.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBarPanelActive;

/// Tells the bar's overlay entity that the panel is its surface right now.
///
/// Every question the host asks about the bar is asked of the overlay entity: is it open, does it
/// own input, should Escape dismiss it. The overlay's own node stays hidden while the panel is up,
/// so without this each of those answers is wrong about a bar the user is looking at.
fn mark_command_bar_shown_inline(
    panel_active: Query<(), With<CommandBarPanelActive>>,
    bar_q: Query<(Entity, Has<OverlayShownInline>), With<CommandBar>>,
    mut commands: Commands,
) {
    let inline = !panel_active.is_empty();
    for (bar, marked) in bar_q.iter() {
        if inline == marked {
            continue;
        }
        if inline {
            commands.entity(bar).insert(OverlayShownInline);
        } else {
            commands.entity(bar).remove::<OverlayShownInline>();
        }
    }
}

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
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_command_bar_panel_active)
            .add_systems(Update, mark_command_bar_shown_inline);
        app
    }

    /// The panel must release the keyboard as reliably as it takes it: a stuck marker leaves the
    /// layout shell owning `CefKeyboardTarget` and no pane can ever get it back.
    #[test]
    fn active_event_round_trips_the_marker() {
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: CommandBarPanelActiveEvent { active: true },
        });
        app.update();
        assert!(app.world().get::<CommandBarPanelActive>(webview).is_some());

        app.world_mut().trigger(BinReceive {
            webview,
            payload: CommandBarPanelActiveEvent { active: false },
        });
        app.update();
        assert!(app.world().get::<CommandBarPanelActive>(webview).is_none());
    }
}
