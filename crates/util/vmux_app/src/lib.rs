#[cfg(feature = "mobile")]
pub use mobile::VmuxMobilePlugin;
#[cfg(feature = "ecs")]
pub use plugin::VmuxEcsPlugin;
#[cfg(all(feature = "runtime", not(target_os = "ios")))]
pub use plugin::VmuxRuntimePlugin;
pub use plugin::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
#[cfg(feature = "cli")]
pub use tool::VmuxToolPlugin;
#[cfg(feature = "ecs")]
pub use vmux_ecs as ecs;
#[cfg(feature = "extension")]
pub use vmux_extension as extension;
#[cfg(feature = "flex")]
pub use vmux_flex as flex;
#[cfg(feature = "layout")]
pub use vmux_layout as layout;
#[cfg(feature = "project")]
pub use vmux_project as project;

#[cfg(feature = "mobile")]
mod mobile;
mod plugin;
pub mod prelude;
#[cfg(feature = "cli")]
mod tool;
