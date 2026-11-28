#[cfg(feature = "window")]
pub use crate::DefaultPlugins;
#[cfg(feature = "ecs")]
pub use crate::MinimalPlugins;
#[cfg(any(feature = "http-server", feature = "quic-server"))]
pub use crate::ServerPlugins;
pub use crate::{VmuxPlugin, VmuxPluginBuilder, VmuxPluginOptions};
pub use bevy_app::{
    App, AppExit, Plugin, PluginGroup, PluginGroupBuilder, PostUpdate, PreStartup, PreUpdate,
    Startup, Update,
};

#[cfg(feature = "ecs")]
pub use crate::VmuxEcsPlugin;
#[cfg(feature = "acp")]
pub use vmux_acp::AcpPlugin;
#[cfg(feature = "contracts")]
pub use vmux_api::{BinEvent, HostEvent, UiEvent, UiEventPermissions, UiStateFrame, UiStatePatch};
#[cfg(feature = "browser")]
pub use vmux_browser::BrowserPlugin;
#[cfg(feature = "ecs")]
pub use vmux_ecs::{
    Dirty, DirtyPlugin, FilePath, Generation, GenerationPlugin, IdentityIndex, IdentityIndexPlugin,
    LeasePlugin, Operation, OperationCompleted, OperationPlugin, OperationPolicy, OperationState,
    Revision, SelectRequest, Selected, SelectionPlugin, ShutdownPlugin, ShutdownRequest,
};
#[cfg(any(feature = "ui", feature = "page-host"))]
pub use vmux_ecs::{
    UiEmitEvent, UiIpcBlocked, UiIpcEpoch, UiIpcEventPlugin, UiIpcGeneration, UiIpcInput,
    UiIpcNavigationRevision, UiIpcPlugin, UiIpcReady, UiPageActivated, UiPageReady, UiReceiveRaw,
    UiState, UiStateEmitSet, UiStateFlushSet, UiStatePlugin, UiStateWrite, UiStateWriter,
};
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
#[cfg(feature = "contracts")]
pub use vmux_macro::{command, contract, host_event, ui_event, ui_state, ui_state_patch};
#[cfg(feature = "menu-native")]
pub use vmux_menu::NativeMenuPlugin;
#[cfg(feature = "menu")]
pub use vmux_menu::{
    Entry as MenuEntry, MenuAction, MenuActionPlugin, MenuBar, MenuPlugin, MenuSet,
    Selected as MenuActionSelected, Selection as MenuSelection, Submenu,
};
#[cfg(feature = "page-host")]
pub use vmux_page::{
    HostedPage, PageAppearance, PageBounds, PageFocusRequest, PageHostMode, PageHostPlugin,
    PageHostSet, PageLifecycle, PageRoot, PageScriptRequest, PageStylesheet,
};
#[cfg(feature = "page")]
pub use vmux_page::{Page, PagePlacement, PagePlugin, PageRegistration, page};
#[cfg(feature = "project")]
pub use vmux_project::ProjectPlugin;
#[cfg(feature = "service")]
pub use vmux_service::plugin::ServicePlugin;
#[cfg(feature = "store")]
pub use vmux_store::{
    AutoSave, AutoSavePlugin, AutoSavePolicy, SchemaPlugin, StorePath, StoreWriteRequest,
    StoreWriteResult,
};
#[cfg(feature = "terminal")]
pub use vmux_terminal::TerminalPlugin;
#[cfg(feature = "ui")]
pub use vmux_ui::hooks::{send as send_ui_event, use_ui_state};
#[cfg(feature = "window")]
pub use vmux_window::{
    CloseWindowRequest, DisplayRelocationPlugin, HideAllWindowsRequest, HostWindow,
    OpenWindowRequest, WindowHostConfig, WindowHostPlugin,
};
