use bevy::prelude::*;

pub mod acp;
pub mod cli;
pub mod page;

/// Wires the agent clients: the ACP bridge and the in-page provider strategies.
pub struct AgentClientPlugin;

impl Plugin for AgentClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((acp::AcpAgentPlugin, page::plugin::PageAgentPlugin));
    }
}
