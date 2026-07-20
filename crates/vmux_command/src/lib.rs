//! The command vocabulary and command-bar wire protocol: the `AppCommand` type, the
//! issue/read system-set ordering, and the snapshots the command bar consumes.

pub mod event;
#[cfg(not(target_arch = "wasm32"))]
pub mod open;
pub mod open_target;
pub mod prompt_media;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod issued;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod shortcut;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot;

#[cfg(not(target_arch = "wasm32"))]
pub use bundle::COMMAND_BAR_PAGE_URL;
#[cfg(not(target_arch = "wasm32"))]
pub use command::*;
#[cfg(not(target_arch = "wasm32"))]
pub use issued::{CommandIssued, CommandIssuer};
#[cfg(not(target_arch = "wasm32"))]
pub use open::*;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::CommandPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use snapshot::*;
