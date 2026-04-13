use crate::command::{AppCommand, ReadAppCommands, TabCommand};
use bevy::prelude::*;

pub(crate) struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_tab_commands.in_set(ReadAppCommands));
    }
}

#[allow(dead_code)]
#[derive(Component)]
pub(crate) struct Tab;


fn handle_tab_commands(mut reader: MessageReader<AppCommand>) {
    for cmd in reader.read() {
        let AppCommand::Tab(tab_cmd) = *cmd else {
            continue;
        };

        match tab_cmd {
            TabCommand::New => {}
            TabCommand::Close => {}
            TabCommand::Next | TabCommand::Previous => {}
        }
    }
}
