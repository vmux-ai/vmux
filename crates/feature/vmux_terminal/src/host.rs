mod agent;
pub mod agent_run;
mod command;
pub mod component;
pub mod contract;
mod input_queue;
pub mod launch;
mod loading;
mod mouse;
pub mod pid;
pub mod plugin;
mod process_control;
pub mod process_monitor;
mod request;
mod service;
pub mod shell_env;
pub mod shell_input;
pub mod snapshot_updater;
mod state;
pub mod theme;
mod tool;

pub(crate) mod link;

pub use agent::{AgentNewTerminalTab, AgentRunShell, AgentShellMode, AgentTerminalSend};
pub use agent_run::{AgentRun, AgentRunWithPlacementOverride, PlacementMode};
pub use component::{
    AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal,
    TerminalUiStateUpdates,
};
pub use contract::TerminalContractPlugin;
pub use plugin::{
    AwaitingProcessCreated, CommandLifecycleEvent, OscTitleChanged, PendingServiceCreate,
    ProcessExitedEvent, RestartPty, TerminalPlugin, TerminalReinputRequest, TerminalRestartRequest,
    TerminalStackSpawnRequest, TerminalStackSpawnSet, has_live_terminal, new_terminal_bundle,
    new_terminal_bundle_with_cwd, reattach_terminal_bundle, should_confirm_close,
};
pub use process_control::TerminalGridSize;
pub use request::{RunShellRequest, ShellMode, TerminalRequestPlugin, TerminalSendRequest};
pub use theme::{TerminalFontSizeCommand, TerminalThemePlugin};
pub use tool::TerminalToolPlugin;
