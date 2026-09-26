pub mod component;
pub mod plugin;
pub use component::*;
pub use plugin::CorePlugin;

pub mod agent;
pub mod archive;
pub mod browser;
pub mod extension;
pub mod file_ui_state;
pub mod host_spawn;
pub mod launcher;
pub mod notify;
pub mod overlay;
pub mod page;
pub mod page_open;
pub mod profile;
pub mod team;
pub mod terminal;
pub mod ui_state;
pub mod wake;
pub mod workspace;

pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
pub use file_ui_state::{FileUiStateUpdates, FileUiStateWrite};
pub use host_spawn::HostSpawnRoute;
pub use launcher::{
    ContributedCommandChosen, HostsLauncher, InlineTransitionRequested, PendingLaunch,
    RendersLauncherPanel, RestoreKeyboardToStack, StackInPaneChosen,
};
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
pub use overlay::{OverlayShownInline, OverlayState, OverlayStateQuery, WindowOverlay};
pub use page_open::{
    CefPageAttachRequest, PageOpenDeferred, PageOpenError, PageOpenHandled, PageOpenId,
    PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask, PendingPrompt,
    PendingPromptAttachments,
};
pub use ui_state::{UiState, UiStatePlugin, UiStateWrite};
pub use workspace::{ComputeFocusSet, StackCommandSet, TabCommandSet};
