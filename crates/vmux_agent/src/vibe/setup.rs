//! Getting an agent installed, from both sides.
//!
//! `event` is the vocabulary the two halves share. `page` renders the setup screen, `plugin` runs
//! the installer and watches for it to finish — one gate each, rather than one per item.

pub mod event;

#[cfg(web)]
pub mod page;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub(crate) use plugin::AgentSetupNavigated;
#[cfg(native)]
pub use plugin::AgentSetupPlugin;
