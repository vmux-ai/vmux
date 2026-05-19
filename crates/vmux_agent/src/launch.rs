use crate::strategy::AgentStrategies;
use crate::{AgentKind, mcp};
use std::path::Path;
use vmux_terminal::launch::TerminalLaunch;

pub fn build_agent_launch(
    kind: AgentKind,
    cwd: &Path,
    session_id: Option<&str>,
    strategies: &AgentStrategies,
) -> Result<TerminalLaunch, String> {
    let strategy = strategies
        .get_cli(kind)
        .ok_or_else(|| format!("CLI strategy not registered for {:?}", kind))?;
    let exe_name = kind.executable();
    let exe_path = crate::exec::find_executable(exe_name)
        .ok_or_else(|| format!("{exe_name} executable not found"))?;
    let mcp_cfg = mcp::resolve(cwd)?;
    let args = strategy.build_args(&mcp_cfg, session_id);
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.extend(strategy.build_env(&mcp_cfg));
    Ok(TerminalLaunch {
        command: exe_path.to_string_lossy().to_string(),
        args,
        cwd: cwd.to_string_lossy().to_string(),
        env,
        kind: kind.into(),
    })
}
