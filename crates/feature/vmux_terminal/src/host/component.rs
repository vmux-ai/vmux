pub use vmux_ecs::terminal::{ProcessExited, PtyExited, Terminal, TerminalUiStateUpdates};

#[derive(bevy::prelude::Component)]
pub struct AgentRunTerminal;

#[derive(bevy::prelude::Component)]
pub struct RetainOnProcessExit;
