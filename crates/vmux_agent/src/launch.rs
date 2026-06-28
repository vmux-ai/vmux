use crate::strategy::AgentStrategies;
use crate::{AgentKind, mcp};
use std::path::Path;
use vmux_core::terminal::TerminalLaunch;

pub fn build_agent_launch(
    kind: AgentKind,
    cwd: &Path,
    session_id: Option<&str>,
    strategies: &AgentStrategies,
    exe_path: &Path,
    anchor: vmux_core::ProcessId,
) -> Result<TerminalLaunch, String> {
    let strategy = strategies
        .get_cli(kind)
        .ok_or_else(|| format!("CLI strategy not registered for {:?}", kind))?;
    let mcp_cfg = mcp::resolve(cwd, anchor)?;
    strategy.prepare_launch(&mcp_cfg);
    let args = strategy.build_args(&mcp_cfg, session_id);
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.extend(strategy.build_env(&mcp_cfg));
    env.push(("VMUX_ANCHOR".to_string(), anchor.to_string()));
    Ok(TerminalLaunch {
        command: exe_path.to_string_lossy().to_string(),
        args,
        cwd: cwd.to_string_lossy().to_string(),
        env,
        kind: kind.into(),
    })
}
