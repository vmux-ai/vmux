#![cfg_attr(ui, allow(non_snake_case))]

#[cfg(host)]
pub use host::{
    AgentInvokeCommand, BindCommands, Binding, ClaimedUrl, ClaimedUrls, CommandBar,
    CommandBarContextSnapshot, CommandBarDismiss, CommandBarNativeSize, CommandBarOpenProjection,
    CommandBarOpenRequest, CommandBarPagesSnapshot, CommandBarPanelActive, CommandBarPlugin,
    CommandBarProjectRoots, CommandBarProjector, CommandBarState, CommandBarWorkDirectory,
    CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot, CommandBinding,
    CommandBindingRegistration, CommandDefinition, CommandDispatch, CommandInvocation,
    CommandManifest, CommandMcp, CommandPlugin, CommandRegistry, CommandRuntimePlugin,
    CommandShortcut, CommandToolPlugin, ContributedAgentModels, ContributedAgentModes,
    ContributedCommand, ContributedPage, ContributedPages, DispatchCommandInvocations, KeyCombo,
    KeyContext, Keymap, Modifiers, PendingCommandBarReveal, ReadCommandRequests, RegisteredPage,
    ResolvedKey, ResolvedLocale, Shortcut, ShortcutDefinition, Source, When,
    WriteCommandBarRequests, WriteCommandBarSnapshots, WriteCommandRequests,
};
pub use palette_surface::CommandPaletteSurface;
#[cfg(ui)]
pub use ui::{CommandBarPanel, CommandPalette, PaletteProps, ResultRow, use_command_bar_ui};
pub use vmux_macro::command;

mod palette_surface_driver;

#[doc(hidden)]
pub mod __private {
    #[cfg(host)]
    pub use crate::host::CommandMessageRegistration;
    pub use inventory;
}

extern crate self as vmux_command;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(ui)]
mod ui;

mod palette_surface;

#[cfg(host)]
mod host;
