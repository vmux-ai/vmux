use crate::strategy::AgentStrategies;
use crate::{AgentKind, mcp};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use vmux_core::terminal::TerminalLaunch;

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

pub(crate) fn build_agent_launch(
    kind: AgentKind,
    cwd: &Path,
    session_id: Option<&str>,
    strategies: &AgentStrategies,
    exe_path: &Path,
    anchor: vmux_core::ProcessId,
    effort: Option<&str>,
    model: Option<&str>,
) -> Result<PreparedAgentLaunch, String> {
    let strategy = strategies
        .get_cli(kind)
        .ok_or_else(|| format!("CLI strategy not registered for {:?}", kind))?;
    let _preparation = AgentLaunchPreparation::lock()?;
    for _ in 0..3 {
        let mcp_revision =
            vmux_core::profile::mcp_credentials::McpOauthCredentials::stable_revision()?;
        let mcp_cfg = mcp::resolve(cwd, anchor, kind)?;
        if let Err(error) = vmux_core::knowledge::sync_external_agent_configs() {
            bevy::log::warn!("external agent Knowledge sync failed: {error}");
        }
        strategy.prepare_launch(&mcp_cfg);
        let effort_key = format!("cli:{}", kind.as_url_segment());
        let mut args = match effort
            .filter(|level| vmux_core::agent::effort_levels(&effort_key).contains(level))
        {
            Some(level) => strategy.effort_args(level),
            None => Vec::new(),
        };
        if let Some(model) = model.filter(|model| !model.is_empty()) {
            args.extend(strategy.model_args(model));
        }
        args.extend(strategy.build_args(&mcp_cfg, session_id));
        let mut env: Vec<(String, String)> = std::env::vars().collect();
        env.extend(strategy.build_env(&mcp_cfg));
        if let Some(model) = model.filter(|model| !model.is_empty()) {
            env.extend(strategy.model_env(model));
        }
        env.push(("VMUX_ANCHOR".to_string(), anchor.to_string()));
        if vmux_core::profile::mcp_credentials::McpOauthCredentials::revision() != mcp_revision {
            continue;
        }
        return Ok(PreparedAgentLaunch {
            launch: TerminalLaunch {
                command: exe_path.to_string_lossy().to_string(),
                args,
                cwd: cwd.to_string_lossy().to_string(),
                env,
                kind: kind.into(),
            },
            mcp_revision,
        });
    }
    Err("MCP configuration changed repeatedly while preparing the agent".to_string())
}
