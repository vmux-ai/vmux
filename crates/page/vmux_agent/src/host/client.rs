use bevy::prelude::*;

pub mod acp;
pub mod cli;
pub mod provider;

pub struct AgentClientPlugin;

impl Plugin for AgentClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((acp::AcpAgentPlugin, provider::ProviderAgentPlugin));
    }
}
