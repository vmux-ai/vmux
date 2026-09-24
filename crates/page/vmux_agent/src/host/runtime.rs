use bevy::prelude::*;

pub mod acp;
pub mod cli;
pub mod provider;

pub struct AgentRuntimePlugin;

impl Plugin for AgentRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((acp::AcpAgentPlugin, provider::ProviderAgentPlugin));
    }
}
