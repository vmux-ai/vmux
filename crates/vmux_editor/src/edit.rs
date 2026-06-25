pub mod buffer;
pub mod command;
pub mod core;
pub mod highlight_cache;

pub use command::{EditCommand, Motion, Selection};
pub use core::{EditCore, EditOutcome};
pub use vmux_core::{CursorPos, EditMode, SelSpan};
