//! The command vocabulary and command-bar wire protocol: the `AppCommand` type, the
//! issue/read system-set ordering, and the snapshots the command bar consumes.

pub mod event;
pub use vmux_wire::open_target;
pub use vmux_wire::prompt_media;

#[cfg(not(web))]
pub mod host;
#[cfg(not(web))]
pub use host::*;
