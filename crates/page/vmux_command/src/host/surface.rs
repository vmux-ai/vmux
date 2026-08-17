//! The bar's host-side entity: state, and no surface of its own.
//!
//! There is no webview here any more. The bar the user opens is `panel::CommandBarPanel`, drawn
//! inside the layout page, and this entity exists so the thirty-odd readers of "is the bar open"
//! keep a single place to ask — `mark_command_bar_shown_inline` puts `OverlayShownInline` on it
//! while the panel is up.
//!
//! It still spawns unparented and declares [`WindowOverlay`], because the layout adopts every
//! unparented overlay into its window root and the overlay vocabulary is what the readers query.

use bevy::prelude::*;
use vmux_core::overlay::WindowOverlay;

use crate::bundle::CommandBar;
use vmux_flex::prelude::*;

pub(crate) struct CommandBarSurfacePlugin;

impl Plugin for CommandBarSurfacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_command_bar_surface);
    }
}

impl CommandBar {
    /// The state the readers query, and nothing that draws.
    ///
    /// The `Node` and `Visibility` stay because `OverlayState::of` reads them; they describe an
    /// overlay that is never shown, which is the truth about this entity.
    fn surface() -> impl Bundle {
        (
            CommandBar,
            WindowOverlay,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                ..default()
            },
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
    use bevy_cef::prelude::{WebviewSource, WebviewWindowed};

    fn spawned() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Startup, spawn_command_bar_surface);
        app.update();
        let bar = app
            .world_mut()
            .query_filtered::<Entity, With<CommandBar>>()
            .single(app.world())
            .expect("command bar entity");
        (app, bar)
    }

    /// The layout adopts overlays by looking for unparented ones, and the readers ask through the
    /// overlay vocabulary, so both have to survive the surface being taken away.
    #[test]
    fn the_entity_spawns_unparented_and_declares_the_capability() {
        let (app, bar) = spawned();
        let entity = app.world().entity(bar);

        assert!(entity.contains::<WindowOverlay>());
        assert!(entity.get::<ChildOf>().is_none());
    }

    /// Giving this entity a webview again would put a browser back in the focus race: it reports
    /// itself open through `OverlayShownInline` while the panel is up, and anything focusable that
    /// says it is open will be focused — taking the keyboard off the page actually drawing the bar.
    #[test]
    fn the_entity_has_no_surface_to_be_focused_instead_of_the_panel() {
        let (app, bar) = spawned();
        let entity = app.world().entity(bar);

        assert!(!entity.contains::<WebviewSource>());
        assert!(!entity.contains::<WebviewWindowed>());
    }
}
