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

    #[test]
    fn the_entity_spawns_unparented_and_declares_the_capability() {
        let (app, bar) = spawned();
        let entity = app.world().entity(bar);

        assert!(entity.contains::<WindowOverlay>());
        assert!(entity.get::<ChildOf>().is_none());
    }

    #[test]
    fn the_entity_has_no_surface_to_be_focused_instead_of_the_panel() {
        let (app, bar) = spawned();
        let entity = app.world().entity(bar);

        assert!(!entity.contains::<WebviewSource>());
        assert!(!entity.contains::<WebviewWindowed>());
    }
}
