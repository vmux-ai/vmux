use bevy_ecs::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct AcpSession {
    pub agent_id: String,
    pub sid: String,
    pub cwd: std::path::PathBuf,
    pub anchor: vmux_wire::ProcessId,
    pub resume: Option<String>,
}
