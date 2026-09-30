#![cfg_attr(ui, allow(non_snake_case))]

extern crate self as vmux_command;

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");

#[cfg(ui)]
pub mod ui;

pub mod palette;
pub mod size;
pub use vmux_api::open_target;
pub use vmux_api::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::{
    AgentAccess, AgentInvokeCommand, AgentPromptTarget, AgentProviderSummary, AgentStrategySummary,
    BindCommands, ClaimedUrl, ClaimedUrls, CommandBar, CommandBarAgentModels, CommandBarAgentModes,
    CommandBarAgentsSnapshot, CommandBarEntry, CommandBarPagesSnapshot, CommandBarPicks,
    CommandBarProjectRoots, CommandBarProjection, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, CommandBarUiStateUpdates, CommandBarWorkDirectory,
    CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot, CommandDefinition, CommandDispatch,
    CommandInvocation, CommandManifest, CommandMcp, CommandMessage, CommandPlugin, CommandRegistry,
    CommandRuntimePlugin, CommandShortcut, CommandToolPlugin, ContributedCommand, ContributedPage,
    ContributedPages, DispatchCommandInvocations, InputSchema, KeyPlugin, ReadCommandRequests,
    RegisteredPage, ResolvedLocale, ShortcutDefinition, SpaceSummary, UiStatePlugin,
    WriteCommandBarSnapshots, WriteCommandRequests, build_command_bar_open_payload, bundle,
    command_bar, command_bar_open_payload, command_list, definition, page_key, payload, plugin,
    settings, shortcut, snapshot, surface,
};
