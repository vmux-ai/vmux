use bevy::prelude::*;
use bevy::window::PrimaryWindow;
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
    orphans: Query<Entity, (With<WindowOverlay>, Without<ChildOf>)>,
    root_q: Query<Entity, With<VmuxWindow>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    if orphans.is_empty() {
        return;
    }
    let Ok(root) = root_q.single() else {
        return;
    };
    let Ok(window) = primary_window.single() else {
        return;
    };
    for overlay in orphans.iter() {
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
            .add_systems(PreUpdate, adopt_window_overlays);
        let root = app.world_mut().spawn(VmuxWindow).id();
        app.world_mut().spawn(PrimaryWindow);
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
    fn an_already_parented_overlay_is_left_alone() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PreUpdate, adopt_window_overlays);
        app.world_mut().spawn(VmuxWindow);
        app.world_mut().spawn(PrimaryWindow);
        let elsewhere = app.world_mut().spawn_empty().id();
        let overlay = app
            .world_mut()
            .spawn((WindowOverlay, ChildOf(elsewhere)))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .entity(overlay)
                .get::<ChildOf>()
                .map(ChildOf::parent),
            Some(elsewhere)
        );
    }
}
