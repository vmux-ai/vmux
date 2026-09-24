pub use vmux_core::terminal::{ProcessExited, PtyExited, Terminal, TerminalUiStateUpdates};

#[derive(bevy::prelude::Component)]
pub struct AgentRunTerminal;

#[derive(bevy::prelude::Component)]
pub struct RetainOnProcessExit;
