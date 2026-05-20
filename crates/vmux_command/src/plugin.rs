use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::snapshot::{
    CommandBarAgentsSnapshot, CommandBarSettingsSnapshot, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, WriteCommandBarSnapshots,
};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarSettingsSnapshot>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .configure_sets(
                Update,
                (WriteAppCommands, WriteCommandBarSnapshots, ReadAppCommands).chain(),
            );
    }
}
