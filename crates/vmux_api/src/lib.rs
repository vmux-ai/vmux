extern crate self as vmux_api;

pub mod agent;
pub mod avatar;
pub mod bin_event;
pub mod bookmark;
pub mod chat;
pub mod command_bar;
pub mod error;
pub mod git;
pub mod history;
pub mod icon;
pub mod input_schema;
pub mod json;
pub mod layout;
pub mod mcp;
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

pub use agent::AgentKind;
pub use bin_event::{BinEvent, BinEventFamily, BinEventTarget, HostEvent, PageReady, UiEvent};
pub use icon::{BuiltinIcon, PageIcon};
pub use input_schema::{InputSchema, InputSchemaType};
pub use page_metadata::{PageIdentity, PageMetadata};
pub use process_id::ProcessId;
pub use route::{InvalidVmuxRoute, VmuxRoute};
pub use terminal::{
    AnsiPalette, CursorShape, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH,
    FLAG_UNDERLINE, LinkRange, RgbColor, TermColor, TermCursor, TermLine, TermSelectionRange,
    TermSpan,
};
pub use ui_state::{UiState, UiStatePatch};
pub use vmux_macro::{
    HostEvent, UiEvent, UiState, UiStatePatch, bidirectional_event, host_event, payload, ui_event,
};
