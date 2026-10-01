mod plugin;
pub use plugin::CommandPlugin;

mod bundle;
mod command_bar;
mod definition;
mod page_key;
mod payload;
mod settings;
mod shortcut;
mod snapshot;
mod surface;
mod tool;

pub use vmux_api::JsonSchema;

pub use bundle::CommandBar;
pub use command_bar::{
    ApplyCommandBarRequests, CommandBarDismiss, CommandBarNativeSize, CommandBarOpenRequest,
    CommandBarPanelActive, CommandBarPlugin, PendingCommandBarReveal, ResumeRows,
    WriteCommandBarRequests,
};
pub use definition::{
    AgentAccess, BindCommands, CommandBinding, CommandDefinition, CommandDispatch,
    CommandInvocation, CommandManifest, CommandMcp, CommandMessage, CommandRegistry,
    CommandRuntimePlugin, CommandShortcut, DispatchCommandInvocations, ReadCommandRequests,
    ShortcutDefinition, WriteCommandRequests,
};
pub use page_key::KeyPlugin;
pub use payload::{
    CommandBarEntry, CommandBarOpenProjection, CommandBarPicks, CommandBarProjector,
};
pub use settings::ResolvedLocale;
pub use shortcut::{
    Binding, KeyCombo, KeyContext, Keymap, Modifiers, ResolvedKey, Shortcut, Source, When,
};
pub use snapshot::{
    ClaimedUrl, ClaimedUrls, CommandBarPagesSnapshot, CommandBarProjectRoots,
    CommandBarSpacesSnapshot, CommandBarState, CommandBarTerminalPage, CommandBarWorkDirectory,
    CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot, ContributedAgentModels,
    ContributedAgentModes, ContributedCommand, ContributedPage, ContributedPages, RegisteredPage,
    SpaceSummary, UiStatePlugin, WriteCommandBarSnapshots,
};
pub use tool::AgentInvokeCommand;
pub use tool::CommandToolPlugin;
