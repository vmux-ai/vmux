pub mod plugin;
pub use plugin::CommandPlugin;

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
    AgentAccess, CommandDefinition, CommandDispatch, CommandInvocation, CommandManifest,
    CommandMcp, CommandRequest, CommandRuntimePlugin, CommandShortcut, CommandTypePlugin,
    DispatchCommandInvocations, ReadCommandRequests, RegisterCommandDefinitions,
    ShortcutDefinition, WriteCommandRequests,
};
pub use issued::{ExLineSubmitted, FileStatusPicked};
pub use page_key::KeyPlugin;
pub use payload::{
    CommandBarEntry, CommandBarPicks, build_command_bar_open_payload, command_bar_open_payload,
    command_list,
};
pub use settings::ResolvedLocale;
pub use snapshot::*;
pub use tool::CommandToolPlugin;
