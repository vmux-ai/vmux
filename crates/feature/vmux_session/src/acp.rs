use bevy_ecs::prelude::*;

use crate::{AgentApprovalPolicy, AgentMessages, AgentRunState, PromptQueue};

#[derive(Component, Clone, Debug)]
#[require(AgentMessages, AgentRunState, AgentApprovalPolicy, PromptQueue)]
pub struct AcpSession {
    pub agent_id: String,
    pub sid: String,
    pub cwd: std::path::PathBuf,
    pub anchor: vmux_api::ProcessId,
    pub resume: Option<String>,
}
