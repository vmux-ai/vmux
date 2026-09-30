use bevy::prelude::*;

pub type TerminalUiStateUpdates = super::UiState<crate::event::TerminalUiState>;

#[derive(Component)]
#[require(TerminalUiStateUpdates)]
pub struct Terminal;

#[derive(Component)]
pub struct ProcessExited;

pub type PtyExited = ProcessExited;

#[derive(Component, Debug, Clone, Reflect, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_core::terminal"]
pub struct TerminalLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy)]
pub enum TerminalSpawnTarget {
    Stack(Entity),
    NewStackInPane(Entity),
    Detached,
}

#[derive(Message, Debug, Clone)]
pub struct TerminalSpawnRequest {
    pub cwd: Option<std::path::PathBuf>,
    pub target: TerminalSpawnTarget,
    pub metadata: Option<crate::PageMetadata>,
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
        };
        assert!(launch.args.is_empty());
    }
}
