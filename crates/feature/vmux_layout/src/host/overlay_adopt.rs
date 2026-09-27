use bevy::prelude::*;
use bevy_cef::prelude::HostWindow;
use vmux_core::overlay::WindowOverlay;

use crate::cef::Browser;
use crate::window::VmuxWindow;

pub(crate) struct OverlayAdoptPlugin;

impl Plugin for OverlayAdoptPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, adopt_window_overlays);
    }
}

fn adopt_window_overlays(
    overlays: Query<(Entity, Option<&HostWindow>, Option<&ChildOf>), With<WindowOverlay>>,
    roots: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    focused_window: Res<crate::window::FocusedWindow>,
    mut commands: Commands,
) {
    let Some(window) = focused_window.0 else {
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

    #[test]
    fn an_unparented_overlay_is_placed_in_the_window_root() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::window::FocusedWindow::default())
            .add_systems(PreUpdate, adopt_window_overlays);
        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<crate::window::FocusedWindow>()
            .0 = Some(window);
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::window::FocusedWindow::default())
            .add_systems(PreUpdate, adopt_window_overlays);
        let old_window = app.world_mut().spawn(Window::default()).id();
        let focused_window = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<crate::window::FocusedWindow>()
            .0 = Some(focused_window);
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
