#![cfg_attr(ui, allow(non_snake_case))]

extern crate self as vmux_command;

#[cfg(ui)]
pub mod ui;

pub mod event;
pub mod palette;
mod search_engine;
pub mod size;
pub use vmux_api::open_target;
pub use vmux_api::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::{
    AgentAccess, AgentPromptTarget, AgentProviderSummary, AgentStrategySummary, ClaimedUrl,
    ClaimedUrls, CommandBar, CommandBarAgentModels, CommandBarAgentModes, CommandBarAgentsSnapshot,
    CommandBarEntry, CommandBarPagesSnapshot, CommandBarPicks, CommandBarProjectRoots,
    CommandBarProjection, CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot,
    CommandBarUiStateUpdates, CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot,
    CommandDefinition, CommandDefinitions, CommandDispatch, CommandInvocation, CommandManifest,
    CommandMcp, CommandMessage, CommandPlugin, CommandRuntimePlugin, CommandShortcut,
    CommandToolPlugin, ContributedCommand, ContributedPage, ContributedPages,
    DispatchCommandInvocations, ExLineSubmitted, FileStatusPicked, InputSchema, KeyPlugin,
    ReadCommandRequests, RegisterCommandDefinitions, RegisteredPage, ResolvedLocale,
    ShortcutDefinition, SpaceSummary, UiStatePlugin, WriteCommandBarSnapshots,
    WriteCommandRequests, build_command_bar_open_payload, bundle, command_bar,
    command_bar_open_payload, command_list, definition, issued, page_key, payload, plugin,
    settings, shortcut, snapshot, surface,
};
