extern crate self as vmux_api;

pub mod agent;
pub mod avatar;
pub mod bin_event;
pub mod chat;
pub mod command_bar;
pub mod error;
pub mod history;
pub mod icon;
pub mod json;
pub mod layout;
pub mod mcp;
pub mod open_target;
#[cfg(feature = "bevy")]
pub mod page;
pub mod process_id;
pub mod prompt_media;
pub mod protocol;
pub mod room;
pub mod route;
pub mod service;
pub mod space;
pub mod team;
pub mod terminal;
pub mod vault;

pub use agent::AgentKind;
pub use bin_event::{BinEvent, BinEventTarget, HostEvent, PageReady, UiEvent};
pub use icon::{BuiltinIcon, PageIcon};
pub use process_id::ProcessId;
pub use route::{InvalidVmuxRoute, VmuxRoute};
pub use terminal::{
    AnsiPalette, CursorShape, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH,
    FLAG_UNDERLINE, LinkRange, RgbColor, TermColor, TermCursor, TermLine, TermSelectionRange,
    TermSpan,
};
pub use vmux_macro::{bidirectional_event, host_event, ui_event};
