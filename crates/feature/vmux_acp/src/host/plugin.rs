use bevy::prelude::*;
use vmux_ecs::PageOpenRequest;
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::notify::{AgentAttention, BellReceived, OsNotify};
use vmux_ecs::persistence::PersistenceAppExt;

use crate::AcpSessionId;
use crate::host::event::{AgentRequestInput, AgentToolCallRequest};

pub struct AcpPlugin;

impl Plugin for AcpPlugin {
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
        app.register_persisted::<AcpSessionId>()
            .add_message::<AgentRequestInput>()
            .add_message::<AgentToolCallRequest>()
            .add_message::<SwapStackSession>()
            .add_message::<BellReceived>()
            .add_message::<AgentAttention>()
            .add_message::<OsNotify>()
            .init_resource::<bevy::ecs::message::Messages<PageOpenRequest>>();
    }
}
