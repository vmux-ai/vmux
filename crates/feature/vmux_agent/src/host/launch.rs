use crate::AgentKind;
use bevy::ecs::system::SystemParam;
use bevy::prelude::Query;
use vmux_core::agent::{AgentDisabledSkillRoot, AgentPromptContribution};
use vmux_core::terminal::TerminalLaunch;

use super::mcp::McpServerConfig;

pub(crate) trait CliLaunchProvider: Send + Sync + 'static {
    const KIND: AgentKind;

    fn arguments(mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;

    fn policy_arguments(_policy: &AgentLaunchPolicy) -> Vec<String> {
        Vec::new()
    }

    fn model_arguments(_model: &str) -> Vec<String> {
        Vec::new()
    }

    fn model_environment(_model: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    fn effort_arguments(_effort: &str) -> Vec<String> {
        Vec::new()
    }

    fn environment(mcp: &McpServerConfig) -> Vec<(String, String)>;

    fn prepare(_mcp: &McpServerConfig) {}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct AgentLaunchPolicy {
    instructions: Vec<String>,
    disabled_skill_roots: Vec<std::path::PathBuf>,
}

#[derive(SystemParam)]
pub(crate) struct AgentLaunchPolicyQuery<'w, 's> {
    instructions: Query<'w, 's, &'static AgentPromptContribution>,
    disabled_skill_roots: Query<'w, 's, &'static AgentDisabledSkillRoot>,
}

impl AgentLaunchPolicyQuery<'_, '_> {
    pub(crate) fn snapshot(&self) -> AgentLaunchPolicy {
        let mut instructions = self
            .instructions
            .iter()
            .map(|instruction| instruction.0.clone())
            .collect::<Vec<_>>();
        instructions.sort();
        instructions.dedup();
        let mut disabled_skill_roots = self
            .disabled_skill_roots
            .iter()
            .map(|root| root.0.clone())
            .collect::<Vec<_>>();
        disabled_skill_roots.sort();
        disabled_skill_roots.dedup();
        AgentLaunchPolicy::new(instructions, disabled_skill_roots)
    }
}

impl AgentLaunchPolicy {
    pub(crate) fn new(
        instructions: Vec<String>,
        disabled_skill_roots: Vec<std::path::PathBuf>,
    ) -> Self {
        Self {
            instructions,
            disabled_skill_roots,
        }
    }

    pub(crate) fn prompt(&self, base: &str) -> String {
        let mut prompt = base.to_string();
        for instruction in &self.instructions {
            if instruction.trim().is_empty() {
                continue;
            }
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str(instruction);
        }
        vmux_core::knowledge::AgentPrompt::from(prompt.as_str()).into_string()
    }

    pub(crate) fn disabled_skill_roots(&self) -> &[std::path::PathBuf] {
        &self.disabled_skill_roots
    }
}

pub(crate) struct PreparedAgentLaunch {
    pub(crate) launch: TerminalLaunch,
    pub(crate) mcp_revision: u64,
}

#[derive(bevy::prelude::Component, Clone)]
pub(crate) struct AgentLaunchRequest {
    pub(crate) cwd: std::path::PathBuf,
    pub(crate) shell: String,
    pub(crate) session_id: Option<String>,
    pub(crate) executable: std::path::PathBuf,
    pub(crate) anchor: vmux_core::ProcessId,
    pub(crate) effort: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) kind: AgentKind,
}

#[derive(bevy::prelude::Component, Clone)]
pub(crate) struct AgentRestartRequest {
    pub(crate) launch: TerminalLaunch,
    pub(crate) shell: String,
    pub(crate) session_id: Option<String>,
    pub(crate) anchor: vmux_core::ProcessId,
    pub(crate) kind: AgentKind,
}
