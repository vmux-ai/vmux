use bevy::prelude::*;

use super::event::*;
use super::handler;

pub struct IslandPlugin;

impl Plugin for IslandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<IslandEvent>()
            .add_message::<SummonCommandBar>()
            .add_message::<IslandPanelShow>()
            .add_message::<IslandPanelHide>()
            .add_message::<IslandPanelResize>()
            .add_message::<IslandPanelDismissed>()
            .add_systems(
                Update,
                (handler::summon_to_expand, handler::drive_island_state).chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_messages() {
        let mut app = App::new();
        app.add_plugins(IslandPlugin)
            .add_systems(Update, |mut s: MessageWriter<SummonCommandBar>| {
                s.write(SummonCommandBar);
            });
        app.update();
    }
}
