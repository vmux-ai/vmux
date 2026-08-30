use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::issued::CommandIssued;
use crate::page_key::PageKeyPlugin;
use crate::snapshot::{CommandBarSnapshotPlugin, WriteCommandBarSnapshots};
use crate::surface::CommandBarSurfacePlugin;
use vmux_core::team::{Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            PageKeyPlugin,
            CommandBarSnapshotPlugin,
            CommandBarSurfacePlugin,
        ))
        .add_message::<AppCommand>()
        .add_message::<CommandIssued>()
        .add_message::<crate::host::ExLineSubmitted>()
        .add_message::<crate::host::FileStatusPicked>()
        .configure_sets(
            Update,
            (WriteAppCommands, WriteCommandBarSnapshots, ReadAppCommands).chain(),
        )
        .add_systems(
            Update,
            log_app_commands
                .after(WriteAppCommands)
                .before(ReadAppCommands),
        );
    }
}

fn log_app_commands(
    mut reader: MessageReader<CommandIssued>,
    profiles: Query<(&Profile, Has<User>)>,
) {
    for ev in reader.read() {
        let who = profiles
            .get(ev.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".to_string());
        info!(target: "vmux_command::app_command", caller = %who, cmd = ?ev.command, "AppCommand");
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
