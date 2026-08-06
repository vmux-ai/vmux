//! Lightweight types serialized between vmux processes and pages.

pub mod chat;
pub mod command_bar;
pub mod history;
pub mod icon;
pub mod layout;
pub mod open_target;
pub mod process_id;
pub mod prompt_media;
pub mod protocol;
pub mod service;
pub mod space;
pub mod team;
pub mod terminal;

pub use icon::{BuiltinIcon, PageIcon};
pub use process_id::ProcessId;
pub use terminal::{
    CursorShape, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH,
    FLAG_UNDERLINE, LinkRange, TermColor, TermCursor, TermLine, TermSelectionRange, TermSpan,
};
