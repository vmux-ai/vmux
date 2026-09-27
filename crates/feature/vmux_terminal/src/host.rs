mod agent;
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
pub(crate) mod process_index;
pub mod process_monitor;
mod prompt;
mod request;
mod service;
pub mod shell_env;
pub mod shell_input;
pub mod snapshot_updater;
mod state;
pub mod target;
pub mod theme;
mod tool;

pub(crate) mod link;

pub use component::{
    AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal,
    TerminalUiStateUpdates,
};
pub use contract::TerminalContractPlugin;
pub use plugin::*;
pub use process_control::TerminalGridSize;
pub use prompt::{BufferedAgentPrompt, PromptCapture};
pub use request::{RunShellRequest, ShellMode, TerminalRequestPlugin, TerminalSendRequest};
pub use theme::{TerminalFontSizeCommand, TerminalThemePlugin};
pub use tool::TerminalToolPlugin;
