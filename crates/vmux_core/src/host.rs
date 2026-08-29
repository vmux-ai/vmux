pub mod component;
pub mod plugin;
pub use component::*;
pub use plugin::CorePlugin;

pub mod agent;
pub mod archive;
pub mod browser;
pub mod extension;
pub mod host_spawn;
pub mod launcher;
pub mod notify;
pub mod overlay;
pub mod page;
pub mod page_open;
pub mod profile;
pub mod team;
pub mod terminal;
pub mod workspace;

pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
pub use host_spawn::{HostSpawnRegistry, register_host_spawn};
pub use launcher::{
    ContributedCommandChosen, HostsLauncher, InlineTransitionRequested, PendingLaunch,
    RendersLauncherPanel, RestoreKeyboardToStack, StackInPaneChosen,
};
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
pub use overlay::{OverlayShownInline, OverlayState, OverlayStateQuery, WindowOverlay};
pub use page_open::{
    CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet,
    PageOpenTarget, PageOpenTask, PendingPrompt, PendingPromptAttachments,
};
pub use workspace::{ComputeFocusSet, StackCommandSet, TabCommandSet};
