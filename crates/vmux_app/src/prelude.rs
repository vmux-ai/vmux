pub use crate::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};

#[cfg(feature = "agent")]
pub use vmux_agent::AgentPlugin;
#[cfg(feature = "browser")]
pub use vmux_browser::BrowserPlugin;
#[cfg(feature = "editor")]
pub use vmux_editor::EditorPlugin;
#[cfg(feature = "git")]
pub use vmux_git::GitPlugin;
#[cfg(feature = "layout")]
pub use vmux_layout::LayoutPlugin;
#[cfg(feature = "service")]
pub use vmux_service::plugin::ServicePlugin;
#[cfg(feature = "terminal")]
pub use vmux_terminal::TerminalPlugin;
