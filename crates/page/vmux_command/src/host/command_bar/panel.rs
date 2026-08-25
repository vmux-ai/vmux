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

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBarPanelActive;

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
