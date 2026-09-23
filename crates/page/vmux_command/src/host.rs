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

pub use vmux_api::InputSchema;

pub use bundle::CommandBar;
pub use definition::{
    AgentAccess, CommandCatalog, CommandDefinition, CommandInvocation, CommandMcp, CommandShortcut,
    DispatchCommandInvocations, ReadCommandRequests, RegisterCommandDefinitions,
    ShortcutDefinition, WriteCommandRequests,
};
pub use issued::{CommandIssuer, ExLineSubmitted, FileStatusPicked};
pub use page_key::{KeyPlugin, ScopedKeys};
pub use payload::{
    CommandBarEntry, CommandBarPicks, build_command_bar_open_payload, command_bar_open_payload,
    command_list, localized_command_name,
};
pub use settings::ResolvedLocale;
pub use snapshot::*;
