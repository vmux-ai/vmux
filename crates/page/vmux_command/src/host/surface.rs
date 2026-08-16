//! The bar's host-side surface, spawned by the crate that owns it.
//!
//! A page crate spawns its own webview — `vmux_terminal`, `vmux_space` and `vmux_setting` all do.
//! The bar used to be the exception, assembled inside the layout's window `setup`, which made the
//! shell the author of a surface it only hosts.
//!
//! It cannot parent itself: the window root is the layout's, and reaching for it would invert the
//! dependency. So it spawns unparented and declares [`WindowOverlay`], and the layout adopts every
//! unparented overlay into its root — the same handoff `maintain_warm_page_pool` already uses,
//! and it lets the shell place the surface without being told whose it is.

use bevy::prelude::*;
use bevy_cef::prelude::{
    CefIgnorePinchZoom, WebviewOpaqueWindowedBackground, WebviewSize, WebviewSource,
    WebviewWindowed, WebviewWindowedNativeFocus,
};
use vmux_core::overlay::WindowOverlay;

use crate::bundle::{COMMAND_BAR_PAGE_URL, CommandBar};
use vmux_flex::prelude::*;

pub(crate) struct CommandBarSurfacePlugin;

impl Plugin for CommandBarSurfacePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::COMMAND_BAR_PAGE_MANIFEST);
        app.add_systems(Startup, spawn_command_bar_surface);
    }
}

impl CommandBar {
    /// Everything about the surface that is the bar's own. The layout adds what only it can know:
    /// the host window, the `Browser` marker, and the parent.
    fn surface() -> impl Bundle {
        (
            (
                CommandBar,
                WindowOverlay,
                // An ordinary windowed surface, framed by the shared `sync_windowed_frames` and
                // focused by the shared route. It is offscreen rendering that forced the host to
                // inject its keystrokes; a native view is handed them by AppKit instead.
                WebviewWindowed,
                WebviewWindowedNativeFocus,
                WebviewOpaqueWindowedBackground,
                CefIgnorePinchZoom,
            ),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                ..default()
            },
            WebviewSource::new(COMMAND_BAR_PAGE_URL),
            WebviewSize(Vec2::new(800.0, 600.0)),
            Transform::default(),
            Visibility::Hidden,
        )
    }
}

fn spawn_command_bar_surface(mut commands: Commands) {
    commands.spawn(CommandBar::surface());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::{WebviewNativeLiquidGlass, WebviewNativeOverlay, WebviewTransparent};

    fn spawned() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Startup, spawn_command_bar_surface);
        app.update();
        let bar = app
            .world_mut()
            .query_filtered::<Entity, With<CommandBar>>()
            .single(app.world())
            .expect("command bar surface");
        (app, bar)
    }

    /// The layout adopts overlays by looking for unparented ones, so a surface that arrives with a
    /// parent is never placed in the window root and never appears.
    #[test]
    fn the_surface_spawns_unparented_and_declares_the_capability() {
        let (app, bar) = spawned();
        let surface = app.world().entity(bar);

        assert!(surface.contains::<WindowOverlay>());
        assert!(surface.get::<ChildOf>().is_none());
    }

    /// A windowed CEF view cannot be transparent on macOS, so the bar is composited as an opaque
    /// native view rather than through the glass overlay. Handing it any of these back reintroduces
    /// the black rectangle that forced the windowed path in the first place.
    #[test]
    fn the_surface_is_opaque_and_windowed_rather_than_composited() {
        let (app, bar) = spawned();
        let surface = app.world().entity(bar);

        assert!(surface.contains::<WebviewOpaqueWindowedBackground>());
        assert!(!surface.contains::<WebviewNativeOverlay>());
        assert!(!surface.contains::<WebviewTransparent>());
        assert!(!surface.contains::<WebviewNativeLiquidGlass>());
    }
}
