pub mod extension;
#[cfg(feature = "mobile")]
mod mobile;
mod options;
mod plugin;
pub mod prelude;

#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
pub use options::VmuxPluginOptions;
#[cfg(feature = "core")]
pub use plugin::VmuxCorePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder};
