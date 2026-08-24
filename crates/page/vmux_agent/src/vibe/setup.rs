pub mod event;

#[cfg(host)]
pub mod plugin;
#[cfg(host)]
pub(crate) use plugin::AgentSetupNavigated;
#[cfg(host)]
pub use plugin::AgentSetupPlugin;
