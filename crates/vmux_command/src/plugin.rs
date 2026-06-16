use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::snapshot::{
    CommandBarAgentsSnapshot, CommandBarPagesSnapshot, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, WriteCommandBarSnapshots, update_pages_snapshot,
};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .configure_sets(
                Update,
                (WriteAppCommands, WriteCommandBarSnapshots, ReadAppCommands).chain(),
            )
            .add_systems(
                Update,
                update_pages_snapshot.in_set(WriteCommandBarSnapshots),
            )
            .add_systems(
                Update,
                log_app_commands
                    .after(WriteAppCommands)
                    .before(ReadAppCommands),
            );
    }
}

fn log_app_commands(mut reader: MessageReader<AppCommand>) {
    for cmd in reader.read() {
        info!(target: "vmux_command::app_command", ?cmd, "AppCommand");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_plugin_logs_app_commands_before_readers() {
        let source = include_str!("plugin.rs");
        let log_needle = ["info!(target: ", "\"vmux_command::app_command\""].concat();
        assert!(source.contains("log_app_commands"));
        assert!(source.contains(".after(WriteAppCommands)"));
        assert!(source.contains(".before(ReadAppCommands)"));
        assert!(source.contains(&log_needle));
    }
}
