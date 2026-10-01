mod agent;
mod agent_run;
mod command;
pub(crate) mod component;
pub(crate) mod contract;
mod input_queue;
pub(crate) mod launch;
mod loading;
mod mouse;
pub(crate) mod pid;
pub(crate) mod plugin;
mod process_control;
pub(crate) mod process_monitor;
mod request;
mod service;
mod shell_env;
pub(crate) mod shell_input;
pub(crate) mod snapshot;
mod state;
pub(crate) mod theme;
mod tool;

pub(crate) mod link;

pub use agent::{AgentNewTerminalTab, AgentRunShell, AgentShellMode, AgentTerminalSend};
pub use agent_run::{
    AgentCwd, AgentRun, AgentRunWithPlacementOverride, AgentTerminalShell, PlacementMode,
};
pub use component::{
    AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal,
    TerminalUiStateUpdates,
};
pub use contract::TerminalContractPlugin;
pub use plugin::{
    AwaitingProcessCreated, CommandLifecycleEvent, OscTitleChanged, PendingServiceCreate,
    ProcessExitedEvent, ReattachedTerminalBundle, RestartPty, TerminalBundle, TerminalPlugin,
    TerminalReinputRequest, TerminalRestartRequest, TerminalStackSpawnRequest,
    TerminalStackSpawnSet, has_live_terminal, should_confirm_close,
};
pub use process_control::TerminalGridSize;
pub use request::{RunShellRequest, ShellMode, TerminalRequestPlugin, TerminalSendRequest};
pub use shell_env::LoginShellEnvironment;
pub use theme::{TerminalFontSizeCommand, TerminalThemePlugin};
pub use tool::TerminalToolPlugin;
