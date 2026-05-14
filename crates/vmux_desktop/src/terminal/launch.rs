use bevy::prelude::*;

#[derive(Component, Debug, Clone, Reflect, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct TerminalLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub kind: TerminalKind,
}

#[derive(Debug, Clone, Reflect, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TerminalKind {
    Plain,
    Vibe,
    Claude,
    Codex,
}

impl From<vmux_agent::AgentKind> for TerminalKind {
    fn from(kind: vmux_agent::AgentKind) -> Self {
        match kind {
            vmux_agent::AgentKind::Vibe => TerminalKind::Vibe,
            vmux_agent::AgentKind::Claude => TerminalKind::Claude,
            vmux_agent::AgentKind::Codex => TerminalKind::Codex,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_launch_plain_construction() {
        let launch = TerminalLaunch {
            command: "/bin/zsh".to_string(),
            args: vec![],
            cwd: "/tmp".to_string(),
            env: vec![],
            kind: TerminalKind::Plain,
        };
        assert_eq!(launch.kind, TerminalKind::Plain);
        assert!(launch.args.is_empty());
    }

    #[test]
    fn terminal_kind_from_agent_kind_maps_each_variant() {
        assert_eq!(
            TerminalKind::from(vmux_agent::AgentKind::Vibe),
            TerminalKind::Vibe
        );
        assert_eq!(
            TerminalKind::from(vmux_agent::AgentKind::Claude),
            TerminalKind::Claude
        );
        assert_eq!(
            TerminalKind::from(vmux_agent::AgentKind::Codex),
            TerminalKind::Codex
        );
    }

    #[test]
    fn terminal_launch_vibe_with_resume_args() {
        let launch = TerminalLaunch {
            command: "/usr/local/bin/vibe".to_string(),
            args: vec!["--trust".into(), "--resume".into(), "abc-123".into()],
            cwd: "/work".to_string(),
            env: vec![("VIBE_MCP_SERVERS".into(), "[]".into())],
            kind: TerminalKind::Vibe,
        };
        assert_eq!(launch.kind, TerminalKind::Vibe);
        assert_eq!(launch.args.len(), 3);
        assert!(launch.env.iter().any(|(k, _)| k == "VIBE_MCP_SERVERS"));
    }
}
