#![cfg_attr(ui, allow(non_snake_case))]

#[cfg(host)]
pub use host::{
    AgentAccess, AgentInvokeCommand, ApplyCommandBarRequests, BindCommands, Binding, ClaimedUrl,
    ClaimedUrls, CommandBar, CommandBarDismiss, CommandBarEntry, CommandBarNativeSize,
    CommandBarOpenProjection, CommandBarOpenRequest, CommandBarPagesSnapshot,
    CommandBarPanelActive, CommandBarPicks, CommandBarPlugin, CommandBarProjectRoots,
    CommandBarProjector, CommandBarSpacesSnapshot, CommandBarState, CommandBarTerminalPage,
    CommandBarWorkDirectory, CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot, CommandBinding,
    CommandDefinition, CommandDispatch, CommandInvocation, CommandManifest, CommandMcp,
    CommandMessage, CommandPlugin, CommandRegistry, CommandRuntimePlugin, CommandShortcut,
    CommandToolPlugin, ContributedAgentModels, ContributedAgentModes, ContributedCommand,
    ContributedPage, ContributedPages, DispatchCommandInvocations, KeyCombo, KeyContext, Keymap,
    Modifiers, PendingCommandBarReveal, ReadCommandRequests, RegisteredPage, ResolvedKey,
    ResolvedLocale, ResumeRows, Shortcut, ShortcutDefinition, Source, SpaceSummary, UiStatePlugin,
    When, WriteCommandBarRequests, WriteCommandBarSnapshots, WriteCommandRequests,
};
pub use palette_surface::CommandPaletteSurface;
#[cfg(ui)]
pub use ui::{CommandBarPanel, CommandPalette, PaletteProps, ResultRow, use_command_bar_ui};
pub use vmux_macro::command;

extern crate self as vmux_command;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(ui)]
mod ui;

mod palette_surface;

#[cfg(host)]
mod host;
