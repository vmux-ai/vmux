#[cfg(feature = "cli")]
pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
#[cfg(feature = "cli")]
pub(crate) type Feature = VmuxCliPlugin;

#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;

#[cfg(feature = "cli")]
pub use cli::VmuxCliPlugin;
#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "core")]
pub use plugin::VmuxCorePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
