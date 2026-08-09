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

#[derive(Message, Debug, Clone)]
pub struct ProcessesMonitorSpawnRequest {
    pub target_stack: Entity,
}

#[cfg(test)]
#[path = "terminal.test.rs"]
mod tests;
