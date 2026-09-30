pub mod plugin;
pub use plugin::CommandPlugin;

mod agent;
pub mod bundle;
pub mod command_bar;
pub mod definition;
pub mod issued;
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
    AgentAccess, BindCommands, CommandDefinition, CommandDispatch, CommandInvocation,
    CommandManifest, CommandMcp, CommandMessage, CommandRegistry, CommandRuntimePlugin,
    CommandShortcut, DispatchCommandInvocations, ReadCommandRequests, ShortcutDefinition,
    WriteCommandRequests,
};
pub use issued::{ExLineSubmitted, FileStatusPicked};
pub use page_key::KeyPlugin;
pub use payload::{
    CommandBarEntry, CommandBarPicks, build_command_bar_open_payload, command_bar_open_payload,
    command_list,
};
pub use settings::ResolvedLocale;
pub use snapshot::{
    AgentPromptTarget, AgentProviderSummary, AgentStrategySummary, ClaimedUrl, ClaimedUrls,
    CommandBarAgentModels, CommandBarAgentModes, CommandBarAgentsSnapshot, CommandBarPagesSnapshot,
    CommandBarProjectRoots, CommandBarProjection, CommandBarSpacesSnapshot,
    CommandBarTerminalsSnapshot, CommandBarUiStateUpdates, CommandBarWorkSnapshot,
    CommandBarWorkspaceSnapshot, ContributedCommand, ContributedPage, ContributedPages,
    RegisteredPage, SpaceSummary, UiStatePlugin, WriteCommandBarSnapshots,
};
pub use tool::AgentInvokeCommand;
pub use tool::CommandToolPlugin;
