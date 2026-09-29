use crate::{AgentKind, mcp};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use vmux_core::profile::mcp_credentials::McpCredentialAccess;
use vmux_core::terminal::TerminalLaunch;

use super::mcp::McpServerConfig;

pub(crate) trait CliLaunchProvider: Send + Sync + 'static {
    const KIND: AgentKind;

    fn arguments(mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;

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

struct AgentLaunchPreparation;

impl AgentLaunchPreparation {
    fn lock() -> Result<MutexGuard<'static, ()>, String> {
        static ACCESS: OnceLock<Mutex<()>> = OnceLock::new();
        ACCESS
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())
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

impl AgentLaunchRequest {
    pub(crate) fn prepare<P: CliLaunchProvider>(&self) -> Result<PreparedAgentLaunch, String> {
        if self.kind != P::KIND {
            return Err(format!(
                "CLI launch provider mismatch: requested {:?}, prepared {:?}",
                self.kind,
                P::KIND
            ));
        }
        let _preparation = AgentLaunchPreparation::lock()?;
        for _ in 0..3 {
            let mcp_revision = McpCredentialAccess::stable_revision()?;
            let mcp_cfg = mcp::resolve(&self.cwd, self.anchor, P::KIND, &self.shell)?;
            if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
                bevy::log::warn!("external agent Knowledge sync failed: {error}");
            }
            P::prepare(&mcp_cfg);
            let effort_key = format!("cli:{}", P::KIND.as_url_segment());
            let mut args = match self
                .effort
                .as_deref()
                .filter(|level| vmux_core::agent::effort_levels(&effort_key).contains(level))
            {
                Some(level) => P::effort_arguments(level),
                None => Vec::new(),
            };
            if let Some(model) = self.model.as_deref().filter(|model| !model.is_empty()) {
                args.extend(P::model_arguments(model));
            }
            args.extend(P::arguments(&mcp_cfg, self.session_id.as_deref()));
            let mut env: Vec<(String, String)> = std::env::vars().collect();
            env.extend(P::environment(&mcp_cfg));
            if let Some(model) = self.model.as_deref().filter(|model| !model.is_empty()) {
                env.extend(P::model_environment(model));
            }
            env.push(("VMUX_ANCHOR".to_string(), self.anchor.to_string()));
            if McpCredentialAccess::revision() != mcp_revision {
                continue;
            }
            return Ok(PreparedAgentLaunch {
                launch: TerminalLaunch {
                    command: self.executable.to_string_lossy().to_string(),
                    args,
                    cwd: self.cwd.to_string_lossy().to_string(),
                    env,
                    kind: P::KIND.into(),
                },
                mcp_revision,
            });
        }
        Err("MCP configuration changed repeatedly while preparing the agent".to_string())
    }
}

#[derive(bevy::prelude::Component, Clone)]
pub(crate) struct AgentRestartRequest {
    pub(crate) launch: TerminalLaunch,
    pub(crate) shell: String,
    pub(crate) session_id: Option<String>,
    pub(crate) anchor: vmux_core::ProcessId,
    pub(crate) kind: AgentKind,
}

impl AgentRestartRequest {
    pub(crate) fn prepare<P: CliLaunchProvider>(&self) -> Result<PreparedAgentLaunch, String> {
        if self.kind != P::KIND {
            return Err(format!(
                "CLI restart provider mismatch: requested {:?}, prepared {:?}",
                self.kind,
                P::KIND
            ));
        }
        for _ in 0..3 {
            let mcp_revision = McpCredentialAccess::stable_revision()?;
            let mcp_cfg = mcp::resolve(
                Path::new(&self.launch.cwd),
                self.anchor,
                P::KIND,
                &self.shell,
            )?;
            let args = P::arguments(&mcp_cfg, self.session_id.as_deref());
            let fresh = P::environment(&mcp_cfg);
            let fresh_keys: std::collections::HashSet<String> =
                fresh.iter().map(|(key, _)| key.clone()).collect();
            let mut env: Vec<(String, String)> = self
                .launch
                .env
                .iter()
                .filter(|(key, _)| {
                    !fresh_keys.contains(key)
                        && !crate::managed_mcp::McpAuthorization::is_environment_variable(key)
                })
                .cloned()
                .collect();
            env.extend(fresh);
            if McpCredentialAccess::revision() != mcp_revision {
                continue;
            }
            return Ok(PreparedAgentLaunch {
                launch: TerminalLaunch {
                    command: self.launch.command.clone(),
                    args,
                    cwd: self.launch.cwd.clone(),
                    env,
                    kind: P::KIND.into(),
                },
                mcp_revision,
            });
        }
        Err("MCP configuration changed repeatedly while preparing the agent restart".to_string())
    }
}
