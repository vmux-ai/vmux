#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod render_model;

pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{
    AgentCwd, AgentNewTerminalTab, AgentRun, AgentRunShell, AgentRunTerminal,
    AgentRunWithPlacementOverride, AgentShellMode, AgentTerminalSend, AgentTerminalShell,
    AwaitingProcessCreated, CommandLifecycleEvent, LoginShellEnvironment, OscTitleChanged,
    PendingServiceCreate, PlacementMode, ProcessExited, ProcessExitedEvent, PtyExited,
    ReattachedTerminalBundle, RestartPty, RetainOnProcessExit, RunShellRequest, ShellMode,
    Terminal, TerminalBundle, TerminalContractPlugin, TerminalFontSizeCommand, TerminalGridSize,
    TerminalPlugin, TerminalReinputRequest, TerminalRequestPlugin, TerminalRestartRequest,
    TerminalSendRequest, TerminalStackSpawnRequest, TerminalStackSpawnSet, TerminalThemePlugin,
    TerminalToolPlugin, TerminalUiStateUpdates, has_live_terminal, should_confirm_close,
};
#[cfg(host)]
pub(crate) use host::{
    contract, launch, pid, plugin, process_monitor, shell_input, snapshot, theme,
};
