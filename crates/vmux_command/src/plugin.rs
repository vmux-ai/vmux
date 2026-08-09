use bevy::prelude::*;

use crate::command::{AppCommand, ReadAppCommands, WriteAppCommands};
use crate::issued::CommandIssued;
use crate::snapshot::{
    CommandBarAgentsSnapshot, CommandBarContributions, CommandBarPagesSnapshot,
    CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot, CommandBarWorkSnapshot,
    WriteCommandBarSnapshots, update_pages_snapshot,
};
use vmux_core::team::{Profile, User};

/// Wires the command protocol: the command messages, the command-bar snapshot resources,
/// and the write -> snapshot -> read system ordering.
pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .add_message::<CommandIssued>()
            .init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarContributions>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .init_resource::<CommandBarWorkSnapshot>()
            .configure_sets(
                Update,
                (WriteAppCommands, WriteCommandBarSnapshots, ReadAppCommands).chain(),
            )
            .add_systems(Startup, update_pages_snapshot)
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
#[path = "plugin.test.rs"]
mod tests;
