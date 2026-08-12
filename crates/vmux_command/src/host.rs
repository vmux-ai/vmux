//! Everything that needs a host to run on, rather than the browser bundle.
//!
//! One `not(web)` gate for the lot, rather than an attribute on each declaration: these modules
//! are built on Bevy, which the wasm page bundle does not link. The crate's public paths are
//! unchanged — `lib.rs` re-exports this module's contents, so `vmux_command::open` still resolves
//! from outside and `crate::open` still resolves from within.

pub mod plugin;
pub use plugin::CommandPlugin;

pub mod bundle;
pub mod command;
pub mod issued;
pub mod open;
pub mod page_key;
pub mod shortcut;
pub mod snapshot;

pub use bundle::COMMAND_BAR_PAGE_URL;
pub use command::*;
pub use issued::{CommandIssued, CommandIssuer};
pub use open::*;
pub use page_key::{PageKeyPlugin, ScopedKeys};
pub use snapshot::*;
