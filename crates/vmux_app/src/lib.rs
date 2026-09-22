pub mod extension;
#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;

#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "core")]
pub use plugin::VmuxCorePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
