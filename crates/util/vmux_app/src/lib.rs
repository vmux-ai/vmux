#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "ecs")]
pub use plugin::VmuxEcsPlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
#[cfg(feature = "cli")]
pub use tool::VmuxToolPlugin;

#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;
#[cfg(feature = "cli")]
mod tool;
