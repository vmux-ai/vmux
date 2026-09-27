pub mod event;

#[cfg(host)]
pub mod plugin;
#[cfg(host)]
pub use plugin::AgentSetupPlugin;
#[cfg(host)]
pub(crate) use plugin::{AgentSetupNavigated, AgentSetupView};
