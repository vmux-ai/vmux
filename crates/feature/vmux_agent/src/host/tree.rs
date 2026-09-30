use bevy::prelude::*;
use vmux_core::agent::SwapStackSession;
use vmux_core::host::persistence::WorkspaceStoreValidator;
use vmux_core::notify::{AgentAttention, BellReceived, OsNotify};
use vmux_core::{HostSpawnRoute, PageOpenRequest};

use crate::event::{AgentRequestInput, AgentToolCallRequest};

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_chat::ChatPlugin,
            super::acp::AcpSessionConfigPlugin,
            super::transcript::ChatTranscriptPlugin,
            crate::runtime::AgentRuntimePlugin,
            vmux_layout::LayoutContractPlugin,
            vmux_editor::ContractPlugin,
            vmux_terminal::TerminalContractPlugin,
            vmux_session::room::RoomPlugin,
            crate::command_bar::CommandBarPlugin,
            super::approval::ApprovalPlugin,
            super::attach::AttachPlugin,
            super::attention::AttentionPlugin,
            super::command::CommandPlugin,
            super::continuation::AgentContinuationPlugin,
        ))
        .add_plugins((
            super::follow::FollowPlugin,
            super::handoff::HandoffPlugin,
            super::ingress::AgentIngressPlugin,
            super::page_open::PageOpenPlugin,
            super::snapshot::SnapshotPlugin,
            super::tidy::TidyPlugin,
            super::toast::ToastPlugin,
        ))
        .add_systems(PreStartup, spawn_agent_store_validator)
        .add_systems(Startup, register_agent_session_route)
        .add_message::<AgentRequestInput>()
        .add_message::<AgentToolCallRequest>()
        .add_message::<SwapStackSession>()
        .add_message::<BellReceived>()
        .add_message::<AgentAttention>()
        .add_message::<OsNotify>()
        .init_resource::<bevy::ecs::message::Messages<PageOpenRequest>>();
    }
}

fn spawn_agent_store_validator(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent workspace-store validator"),
        WorkspaceStoreValidator {
            name: "agent URL",
            rejects: crate::AgentUrl::rejects_persisted_store,
        },
    ));
}

fn register_agent_session_route(mut commands: Commands) {
    commands.spawn(HostSpawnRoute::subtree("vmux://sessions/"));
}
