use std::collections::BTreeSet;

use crate::cef::Browser;
use crate::event::LayoutOverlayEvent;
use crate::window::VmuxWindow;
use bevy::prelude::*;
use bevy_cef::prelude::{HostWindow, UiEventPlugin, UiInput};
use vmux_core::overlay::WindowOverlay;

pub(crate) struct LayoutOverlayPlugin;

impl Plugin for LayoutOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(LayoutOverlayEvent,)>::default())
            .add_observer(on_emit)
            .add_systems(PreUpdate, adopt_window_overlays);
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutOverlayActive(BTreeSet<String>);

fn on_emit(
    trigger: On<UiInput<LayoutOverlayEvent>>,
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

fn adopt_window_overlays(
    overlays: Query<(Entity, Option<&HostWindow>, Option<&ChildOf>), With<WindowOverlay>>,
    roots: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    focused_window: crate::window::FocusedWindow,
    mut commands: Commands,
) {
    let Some(window) = focused_window.entity() else {
        return;
    };
    let Some(root) = roots
        .iter()
        .find_map(|(root, host)| (host.0 == window).then_some(root))
    else {
        return;
    };
    for (overlay, host, parent) in &overlays {
        if host.is_some_and(|host| host.0 == window)
            && parent.is_some_and(|parent| parent.parent() == root)
        {
            continue;
        }
        commands
            .entity(overlay)
            .insert((Browser, HostWindow(window), ChildOf(root)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PreUpdate, adopt_window_overlays);
        app
    }

    #[test]
    fn an_unparented_overlay_is_placed_in_the_window_root() {
        let mut app = app();
        let window = app
            .world_mut()
            .spawn((Window::default(), vmux_core::Active))
            .id();
        let root = app.world_mut().spawn((VmuxWindow, HostWindow(window))).id();
        let overlay = app.world_mut().spawn(WindowOverlay).id();

        app.update();

        let overlay_ref = app.world().entity(overlay);
        assert_eq!(
            overlay_ref.get::<ChildOf>().map(ChildOf::parent),
            Some(root)
        );
        assert!(overlay_ref.contains::<Browser>());
    }

    #[test]
    fn overlay_moves_to_the_focused_window() {
        let mut app = app();
        let old_window = app.world_mut().spawn(Window::default()).id();
        let focused_window = app
            .world_mut()
            .spawn((Window::default(), vmux_core::Active))
            .id();
        let elsewhere = app.world_mut().spawn_empty().id();
        let root = app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(focused_window)))
            .id();
        let overlay = app
            .world_mut()
            .spawn((WindowOverlay, HostWindow(old_window), ChildOf(elsewhere)))
            .id();

        app.update();

        let overlay_ref = app.world().entity(overlay);
        assert_eq!(
            overlay_ref.get::<ChildOf>().map(ChildOf::parent),
            Some(root)
        );
        assert_eq!(
            overlay_ref.get::<HostWindow>(),
            Some(&HostWindow(focused_window))
        );
    }
}
