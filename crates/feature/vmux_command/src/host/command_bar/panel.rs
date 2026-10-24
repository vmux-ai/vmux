use bevy::prelude::*;

use crate::CommandBar;
use vmux_api::command_bar::{CommandBarOpenEvent, UiCommandBar, UiCommandBarPatch};
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::RendersLauncherPanel;
use vmux_ecs::overlay::OverlayShownInline;

pub(super) struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(active).add_systems(Update, sync_inline);
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBarPanelActive;

fn sync_inline(
    panel_active: Query<(), (With<CommandBarPanelActive>, With<RendersLauncherPanel>)>,
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

fn active(
    trigger: On<UiStateWrite<UiCommandBar>>,
    panels: Query<(), With<RendersLauncherPanel>>,
    mut commands: Commands,
) {
    let Some(opened) = <UiCommandBarPatch as vmux_api::UiStatePatch<CommandBarOpenEvent>>::payload(
        trigger.event().update(),
    ) else {
        return;
    };
    if !panels.contains(trigger.event().webview()) {
        return;
    }
    let Ok(mut webview) = commands.get_entity(trigger.event().webview()) else {
        return;
    };
    if opened.open_id.is_open() {
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
            .add_observer(active)
            .add_systems(Update, sync_inline);
        app
    }

    #[test]
    fn active_event_round_trips_the_marker() {
        let mut app = app();
        let webview = app.world_mut().spawn(RendersLauncherPanel).id();

        app.world_mut()
            .trigger(UiStateWrite::<UiCommandBar>::from_event(
                webview,
                &CommandBarOpenEvent {
                    open_id: vmux_api::command_bar::OpenId(1),
                    ..Default::default()
                },
            ));
        app.update();
        assert!(app.world().get::<CommandBarPanelActive>(webview).is_some());

        app.world_mut()
            .trigger(UiStateWrite::<UiCommandBar>::from_event(
                webview,
                &CommandBarOpenEvent::default(),
            ));
        app.update();
        assert!(app.world().get::<CommandBarPanelActive>(webview).is_none());
    }

    #[test]
    fn inline_launcher_does_not_capture_the_layout() {
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();

        app.world_mut()
            .trigger(UiStateWrite::<UiCommandBar>::from_event(
                webview,
                &CommandBarOpenEvent {
                    open_id: vmux_api::command_bar::OpenId(1),
                    ..Default::default()
                },
            ));
        app.update();

        assert!(app.world().get::<CommandBarPanelActive>(webview).is_none());
    }
}
