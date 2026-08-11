pub mod buffer;
pub mod command;
pub mod core;
pub mod ex;
pub mod highlight_cache;
pub mod register;
pub mod search;
pub mod text_object;
pub mod undo;

pub use command::{EditCommand, Motion, Operator, Selection, Target};
pub use core::{EditCore, EditOutcome};
pub use register::{RegisterKind, RegisterValue, Registers};
pub use text_object::{TextObject, TextObjectKind};
pub use vmux_core::{CursorPos, EditMode, SelSpan};
