use bevy::prelude::*;
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::host::persistence::WorkspaceStoreValidator;
use vmux_ecs::notify::{AgentAttention, BellReceived, OsNotify};
use vmux_ecs::{HostSpawnRoute, PageOpenRequest};

use super::acp::AcpSessionConfigPlugin;
use super::approval::Plugin as ApprovalPlugin;
use super::attach::AttachPlugin;
use super::attention::AttentionPlugin;
use super::command::CommandPlugin;
use super::continuation::AgentContinuationPlugin;
use super::follow::FollowPlugin;
use super::handoff::Plugin as HandoffPlugin;
use super::ingress::AgentIngressPlugin;
use super::page::PagePlugin;
use super::snapshot::SnapshotPlugin;
use super::tidy::Plugin as TidyPlugin;
use super::toast::ToastPlugin;
use crate::host::command_bar::CommandBarPlugin;
use crate::host::event::{AgentRequestInput, AgentToolCallRequest};
use crate::host::runtime::AgentRuntimePlugin;

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AcpSessionConfigPlugin,
            AgentRuntimePlugin,
            CommandBarPlugin,
            ApprovalPlugin,
            AttachPlugin,
            AttentionPlugin,
            CommandPlugin,
            AgentContinuationPlugin,
        ))
        .add_plugins((
            FollowPlugin,
            HandoffPlugin,
            AgentIngressPlugin,
            PagePlugin,
            SnapshotPlugin,
            TidyPlugin,
            ToastPlugin,
        ))
        .add_systems(PreStartup, spawn_store_validator)
        .add_systems(Startup, register_session_route)
        .add_message::<AgentRequestInput>()
        .add_message::<AgentToolCallRequest>()
        .add_message::<SwapStackSession>()
        .add_message::<BellReceived>()
        .add_message::<AgentAttention>()
        .add_message::<OsNotify>()
        .init_resource::<bevy::ecs::message::Messages<PageOpenRequest>>();
    }
}

fn spawn_store_validator(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent workspace-store validator"),
        WorkspaceStoreValidator {
            name: "agent URL",
            rejects: crate::acp::route::AcpRoute::rejects_persisted_store,
        },
    ));
}

fn register_session_route(mut commands: Commands) {
    commands.spawn(HostSpawnRoute::subtree(vmux_chat::ChatPlugin::URL));
}
