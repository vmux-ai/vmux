pub mod plugin;
pub use plugin::CommandPlugin;

pub mod bundle;
pub mod command_bar;
pub mod definition;
pub mod page_key;
pub mod payload;
pub mod settings;
pub mod shortcut;
pub mod snapshot;
pub mod surface;
mod tool;

pub use vmux_api::InputSchema;

pub use bundle::CommandBar;
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
pub use snapshot::{
    AgentPromptTarget, AgentSummary, ClaimedUrl, ClaimedUrls, CommandBarAgentModels,
    CommandBarAgentModes, CommandBarAgentsSnapshot, CommandBarPagesSnapshot,
    CommandBarProjectRoots, CommandBarProjection, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, CommandBarUiStateUpdates, CommandBarWorkDirectory,
    CommandBarWorkSnapshot, CommandBarWorkspaceSnapshot, ContributedCommand, ContributedPage,
    ContributedPages, RegisteredPage, SpaceSummary, UiStatePlugin, WriteCommandBarSnapshots,
};
pub use tool::AgentInvokeCommand;
pub use tool::CommandToolPlugin;
