pub use crate::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};

#[cfg(feature = "acp")]
pub use vmux_acp::AcpPlugin;
#[cfg(feature = "browser")]
pub use vmux_browser::BrowserPlugin;
#[cfg(feature = "editor")]
pub use vmux_editor::EditorPlugin;
#[cfg(feature = "extension")]
pub use vmux_extension::ExtensionHostPlugin;
#[cfg(feature = "extension-core")]
pub use vmux_extension::ExtensionPlugin;
#[cfg(feature = "flex")]
pub use vmux_flex::{ComputedNode, FlexPlugin, FlexViewport, LayoutContext, Node, PrimaryViewport};
#[cfg(feature = "git")]
pub use vmux_git::GitPlugin;
#[cfg(feature = "jobs")]
pub use vmux_job::{
    Job, JobOwner, JobPlugin, JobReady, JobRequest, JobResult, JobSet, JobWake, JobsPlugin,
};
#[cfg(feature = "layout")]
pub use vmux_layout::{
    Hierarchy as PaneHierarchy, LayoutPlugin, Pane, PaneId, PaneSet, PaneSize, PaneSplit,
    PaneSplitDirection, SelectRequest as PaneSelectRequest,
    SelectionChanged as PaneSelectionChanged,
};
#[cfg(feature = "lsp")]
pub use vmux_lsp::LspPlugin;
#[cfg(feature = "menu-native")]
pub use vmux_menu::NativeMenuPlugin;
#[cfg(feature = "menu")]
pub use vmux_menu::{
    Entry as MenuEntry, MenuAction, MenuActionPlugin, MenuBar, MenuPlugin, MenuSet,
    Selected as MenuActionSelected, Selection as MenuSelection, Submenu,
};
#[cfg(feature = "project")]
pub use vmux_project::ProjectPlugin;
#[cfg(feature = "service")]
pub use vmux_service::plugin::ServicePlugin;
#[cfg(feature = "terminal")]
pub use vmux_terminal::TerminalPlugin;
