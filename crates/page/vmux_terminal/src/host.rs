pub mod component;
pub mod contract;
pub mod launch;
pub mod pid;
pub mod plugin;
pub mod processes_monitor;
pub mod shell_env;
pub mod shell_input;
pub mod snapshot_updater;
pub mod target;

pub(crate) mod link;

pub use component::{AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal};
pub use contract::TerminalContractPlugin;
pub use plugin::*;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "terminal",
    title: "Terminal",
    title_message_id: Some("command-terminal"),
    replaces_command: None,
    keywords: &["shell", "console"],
    icon: Some(vmux_core::BuiltinIcon::Terminal),
    command_bar: true,
};
