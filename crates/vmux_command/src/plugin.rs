use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));
    }
}
