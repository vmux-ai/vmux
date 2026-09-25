pub mod extension;
#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
#[cfg(feature = "mcp")]
mod tool;
#[cfg(all(test, feature = "tools"))]
mod tool_tests;
pub mod prelude;

#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "core")]
pub use plugin::VmuxCorePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
#[cfg(feature = "mcp")]
pub use tool::ToolPlugin;
