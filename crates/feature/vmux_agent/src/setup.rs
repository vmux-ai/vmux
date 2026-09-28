pub mod event;

#[cfg(all(host, feature = "app"))]
pub mod plugin;
#[cfg(all(host, feature = "app"))]
pub use plugin::AgentSetupPlugin;
#[cfg(all(host, feature = "app"))]
pub(crate) use plugin::{AgentSetupNavigated, AgentSetupView};
