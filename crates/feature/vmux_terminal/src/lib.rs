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
    PtyExited, ReattachedTerminalBundle, RestartPty, RetainOnProcessExit, RunShellRequest,
    ShellMode, Terminal, TerminalBundle, TerminalContractPlugin, TerminalFontSizeCommand,
    TerminalGridSize, TerminalPlugin, TerminalReinputRequest, TerminalRequestPlugin,
    TerminalRestartRequest, TerminalSendRequest, TerminalStackSpawnRequest, TerminalStackSpawnSet,
    TerminalThemePlugin, TerminalToolPlugin, TerminalUiStateUpdates, agent_run, component,
    contract, has_live_terminal, launch, pid, plugin, process_monitor, shell_env, shell_input,
    should_confirm_close, snapshot_updater, theme,
};
