#![cfg_attr(ui, allow(non_snake_case))]

extern crate self as vmux_command;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(ui)]
pub mod ui;

mod palette_surface;
pub mod size;
pub use palette_surface::CommandPaletteSurface;
pub use vmux_api::open_target;
pub use vmux_api::prompt_media;
pub use vmux_macro::command;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::{
    AgentAccess, AgentInvokeCommand, BindCommands, ClaimedUrl, ClaimedUrls, CommandBar,
    CommandBarAgentModels, CommandBarAgentModes, CommandBarEntry, CommandBarOpenProjection,
    CommandBarPagesSnapshot, CommandBarPicks, CommandBarProjectRoots, CommandBarProjection,
    CommandBarProjector, CommandBarSpacesSnapshot, CommandBarTerminalsSnapshot,
    CommandBarUiStateUpdates, CommandBarWorkDirectory, CommandBarWorkSnapshot,
    CommandBarWorkspaceSnapshot, CommandBinding, CommandDefinition, CommandDispatch,
    CommandInvocation, CommandManifest, CommandMcp, CommandMessage, CommandPlugin, CommandRegistry,
    CommandRuntimePlugin, CommandShortcut, CommandToolPlugin, ContributedCommand, ContributedPage,
    ContributedPages, DispatchCommandInvocations, JsonSchema, KeyPlugin, ReadCommandRequests,
    RegisteredPage, ResolvedLocale, ShortcutDefinition, SpaceSummary, UiStatePlugin,
    WriteCommandBarSnapshots, WriteCommandRequests, bundle, command_bar, definition, page_key,
    payload, plugin, settings, shortcut, snapshot, surface,
};
