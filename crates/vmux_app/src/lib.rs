pub mod extension;
#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;
#[cfg(feature = "mcp")]
mod tool;
#[cfg(all(test, feature = "tools"))]
mod tool_tests;

#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "core")]
pub use plugin::VmuxCorePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
#[cfg(feature = "mcp")]
pub use tool::ToolPlugin;
