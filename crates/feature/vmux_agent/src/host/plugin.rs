use bevy::prelude::*;
use vmux_ecs::PageOpenRequest;
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::host_spawn::HostSpawnRoute;
use vmux_ecs::notify::{AgentAttention, BellReceived, OsNotify};
use vmux_ecs::persistence::WorkspaceStoreValidator;

use crate::host::event::{AgentRequestInput, AgentToolCallRequest};
use crate::route::AcpRoute;

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        super::acp::add_config(app);
        super::runtime::add(app);
        super::command_bar::add(app);
        super::approval::add(app);
        super::attach::add(app);
        super::attention::add(app);
        super::command::add(app);
        super::continuation::add(app);
        super::follow::add(app);
        super::handoff::add(app);
        super::ingress::add(app);
        super::navigation::add(app);
        super::tidy::add(app);
        super::toast::add(app);
        app.add_systems(PreStartup, spawn_store_validator)
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
            rejects: AcpRoute::rejects_persisted_store,
        },
    ));
}

fn register_session_route(mut commands: Commands) {
    commands.spawn(HostSpawnRoute::subtree(vmux_api::VmuxRoute::SESSIONS_ROOT));
}
