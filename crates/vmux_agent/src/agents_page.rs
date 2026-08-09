//! The `vmux://agents` manager page: browse the ACP registry catalog (all agents, with icons,
//! descriptions, and the runtime each needs). Install/spawn happens by opening an agent from the
//! launcher; this page is discovery.

pub mod event;

#[cfg(frontend)]
pub mod page;
#[cfg(frontend)]
mod state;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub use plugin::*;
