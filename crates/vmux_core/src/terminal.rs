use bevy::prelude::*;

#[derive(Component)]
pub struct Terminal;

#[derive(Component)]
pub struct ProcessExited;

pub type PtyExited = ProcessExited;

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

#[derive(Message, Debug, Clone)]
pub struct TerminalSpawnRequest {
    pub cwd: Option<std::path::PathBuf>,
    pub target_stack: Option<Entity>,
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
