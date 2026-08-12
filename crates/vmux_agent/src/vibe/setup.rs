//! Getting an agent installed, from both sides.
//!
//! `event` is the vocabulary the two halves share. `page` renders the setup screen, `plugin` runs
//! the installer and watches for it to finish — one gate each, rather than one per item.

pub mod event;

#[cfg(web)]
pub mod page;

#[cfg(host)]
pub mod plugin;
#[cfg(host)]
pub(crate) use plugin::AgentSetupNavigated;
#[cfg(host)]
pub use plugin::AgentSetupPlugin;
