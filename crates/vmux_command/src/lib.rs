pub mod event;
#[cfg(not(target_arch = "wasm32"))]
pub mod open;
pub mod open_target;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
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
pub use open::*;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::CommandPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use snapshot::*;
