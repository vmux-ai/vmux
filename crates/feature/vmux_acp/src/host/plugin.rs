use bevy::prelude::*;
use vmux_ecs::PageOpenRequest;
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::notify::{AgentAttention, BellReceived, OsNotify};
use vmux_ecs::store::StoreAppExt;

use crate::AcpSessionId;
use crate::host::event::{AgentRequestInput, AgentToolCallRequest};

pub struct AcpPlugin;

impl Plugin for AcpPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::acp::ConfigPlugin,
            super::runtime::RuntimePlugin,
            super::command_bar::CommandBarPlugin,
            super::approval::ApprovalPlugin,
            super::attach::AttachPlugin,
            super::attention::AttentionPlugin,
            super::command::CommandPlugin,
            super::continuation::ContinuationPlugin,
            super::external::ExternalPlugin,
            super::follow::FollowPlugin,
            super::handoff::HandoffPlugin,
            super::ingress::IngressPlugin,
            super::navigation::NavigationPlugin,
            super::tidy::TidyPlugin,
            super::toast::ToastPlugin,
        ))
        .register_stored::<AcpSessionId>()
        .add_message::<AgentRequestInput>()
        .add_message::<AgentToolCallRequest>()
        .add_message::<SwapStackSession>()
        .add_message::<BellReceived>()
        .add_message::<AgentAttention>()
        .add_message::<OsNotify>()
        .init_resource::<bevy::ecs::message::Messages<PageOpenRequest>>();
    }
}
