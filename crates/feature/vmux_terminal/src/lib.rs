#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod render_model;

pub mod ui;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::{
    AgentNewTerminalTab, AgentRun, AgentRunShell, AgentRunTerminal, AgentRunWithPlacementOverride,
    AgentShellMode, AgentTerminalSend, AwaitingProcessCreated, CommandLifecycleEvent,
    OscTitleChanged, PendingServiceCreate, PlacementMode, ProcessExited, ProcessExitedEvent,
    PtyExited, RestartPty, RetainOnProcessExit, RunShellRequest, ShellMode, Terminal,
    TerminalContractPlugin, TerminalFontSizeCommand, TerminalGridSize, TerminalPlugin,
    TerminalReinputRequest, TerminalRequestPlugin, TerminalRestartRequest, TerminalSendRequest,
    TerminalStackSpawnRequest, TerminalStackSpawnSet, TerminalThemePlugin, TerminalToolPlugin,
    TerminalUiStateUpdates, agent_run, component, contract, has_live_terminal, launch,
    new_terminal_bundle, new_terminal_bundle_with_cwd, pid, plugin, process_monitor,
    reattach_terminal_bundle, shell_env, shell_input, should_confirm_close, snapshot_updater,
    theme,
};
