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
        app.add_plugins((super::acp::ConfigPlugin, super::runtime::Plugin));
        super::command_bar::add(app);
        app.add_plugins(super::onboarding::Plugin);
        #[cfg(target_os = "macos")]
        app.add_plugins(super::credentials::Plugin);
        app.add_plugins(super::approval::Plugin);
        app.add_plugins(super::attach::Plugin);
        super::attention::add(app);
        app.add_plugins(super::command::Plugin);
        super::continuation::add(app);
        app.add_plugins(super::external::Plugin);
        super::follow::add(app);
        super::handoff::add(app);
        app.add_plugins(super::ingress::Plugin);
        app.add_plugins(super::navigation::Plugin);
        super::tidy::add(app);
        super::toast::add(app);
        app.register_stored::<AcpSessionId>()
            .add_message::<AgentRequestInput>()
            .add_message::<AgentToolCallRequest>()
            .add_message::<SwapStackSession>()
            .add_message::<BellReceived>()
            .add_message::<AgentAttention>()
            .add_message::<OsNotify>()
            .init_resource::<bevy::ecs::message::Messages<PageOpenRequest>>();
    }
}
