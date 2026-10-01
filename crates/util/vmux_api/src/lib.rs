pub use bin_event::{AgentRequestContract, BinEvent, HostEvent, PageReady, UiEvent};
pub use icon::{BuiltinIcon, PageIcon};
pub use json_schema::{JsonSchema, JsonSchemaType};
#[cfg(feature = "bevy")]
pub use page::UiEventPermissions;
pub use page_metadata::{PageIdentity, PageMetadata};
pub use process_id::ProcessId;
pub use route::{InvalidVmuxRoute, VmuxRoute};
pub use terminal::{
    AnsiPalette, CursorShape, CursorStyle, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC,
    FLAG_STRIKETHROUGH, FLAG_UNDERLINE, LinkRange, RgbColor, TermColor, TermCursor, TermLine,
    TermSelectionRange, TermSpan,
};
pub use ui_state::{BatchedUiState, UiState, UiStatePatch};
pub use vmux_macro::{
    agent, bidirectional_event, contract, host_event, service_message, ui_event, ui_event_variants,
    ui_state, ui_state_patch,
};

extern crate self as vmux_api;

pub mod avatar;
pub mod bin_event;
pub mod bookmark;
pub mod chat;
pub mod command_bar;
pub mod editor;
pub mod error;
pub mod extension;
pub mod git;
pub mod history;
pub mod icon;
pub mod input;
pub mod json;
pub mod json_schema;
pub mod knowledge;
pub mod layout;
pub mod mcp;
pub mod media;
pub mod open_target;
#[cfg(feature = "bevy")]
pub mod page;
pub mod page_metadata;
pub mod process_id;
pub mod prompt_media;
pub mod protocol;
pub mod room;
pub mod route;
pub mod service;
pub mod space;
pub mod team;
pub mod terminal;
mod ui_state;
pub mod vault;
