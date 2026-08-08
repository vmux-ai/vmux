//! The command vocabulary and command-bar wire protocol: the `AppCommand` type, the
//! issue/read system-set ordering, and the snapshots the command bar consumes.

pub mod event;
#[cfg(not(web))]
pub mod open;
pub use vmux_wire::open_target;
pub use vmux_wire::prompt_media;

#[cfg(not(web))]
pub mod bundle;
#[cfg(not(web))]
pub mod command;
#[cfg(not(web))]
pub mod issued;
#[cfg(not(web))]
pub mod plugin;
#[cfg(not(web))]
pub mod shortcut;
#[cfg(not(web))]
pub mod snapshot;

#[cfg(not(web))]
pub use bundle::COMMAND_BAR_PAGE_URL;
#[cfg(not(web))]
pub use command::*;
#[cfg(not(web))]
pub use issued::{CommandIssued, CommandIssuer};
#[cfg(not(web))]
pub use open::*;
#[cfg(not(web))]
pub use plugin::CommandPlugin;
#[cfg(not(web))]
pub use snapshot::*;
