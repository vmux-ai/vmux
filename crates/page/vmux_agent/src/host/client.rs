use bevy::prelude::*;

pub mod acp;
pub mod cli;
pub mod page;

pub struct AgentClientPlugin;

impl Plugin for AgentClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((acp::AcpAgentPlugin, page::plugin::PageAgentPlugin));
    }
}
