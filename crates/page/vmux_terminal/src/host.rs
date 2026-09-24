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
pub mod processes_monitor;
mod prompt;
mod request;
pub mod shell_env;
pub mod shell_input;
pub mod snapshot_updater;
pub mod target;
pub mod theme;

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

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "terminal",
    title: "Terminal",
    title_message_id: Some("command-terminal"),
    replaces_command: None,
    keywords: &["shell", "console"],
    icon: Some(vmux_core::BuiltinIcon::Terminal),
    command_bar: true,
};
