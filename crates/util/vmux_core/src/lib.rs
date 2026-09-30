pub mod agent_setup;
pub mod chat;
pub mod chat_projection;
#[cfg(host)]
pub mod cli;
pub mod dom_snapshot;
pub mod editor;
pub mod event;
pub mod executable;
pub mod file_url;
pub mod icon;
pub mod input;
pub mod knowledge;
pub mod language_icon;
pub mod media;
pub mod page_metadata;
pub mod process_id;
pub mod prompt_media;
pub mod room;
pub mod scroll;
pub mod service;
pub mod smart_bookmark_folder;
pub mod tool;
pub mod vault;
pub use editor::{CursorPos, EditMode, KeymapKind, SelSpan};
pub use executable::Executable;
pub use icon::{BuiltinIcon, PageIcon};
pub use input::{KeyModifiers, KeyStroke};
pub use language_icon::LanguageIconPath;
pub use page_metadata::{PageIdentity, PageMetadata};
pub use process_id::ProcessId;
pub use smart_bookmark_folder::SmartBookmarkFolder;
pub use vmux_macro::service_message;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::{
    ActivateRequest, Active, AgentAttention, AgentDoneUnseen, AgentWorkingDir, ArchivedPage,
    ArchivedPagePosition, ArchivedTabPage, Bookmark, BookmarkOrder, CefPageAttachRequest,
    Collapsed, ComputeFocusSet, ContributedCommandChosen, CorePlugin, CreatedAt,
    EffectiveStartupUrl, EntityTarget, FileUiStateUpdates, FileUiStateWrite, Folder, HostShell,
    HostSpawnRoute, HostsLauncher, InlineTransitionRequested, JsonArguments, KeyboardOwner,
    LastActivatedAt, LastVisitedAt, LauncherDismissRequest, Order, OsNotify, OverlayShownInline,
    OverlayState, OverlayStateQuery, PageArchiveRequest, PageOpenDeferred, PageOpenError,
    PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask,
    PaneStep, PendingPrompt, PendingPromptAttachments, Pin, ProcessAnchor, Ready,
    RegistrationOrder, RendersLauncherPanel, RestoreKeyboardToStack, SplitAxis, StackCommandSet,
    StackInPaneChosen, TabCommandSet, TransitionType, UiState, UiStatePlugin, UiStateWrite, Url,
    Uuid, Visit, VisitCount, VisitedUrl, WindowFullscreen, WindowFullscreenSet, WindowOverlay,
    agent, archive, browser, component, file_ui_state, host_spawn, launcher, manifest, notify,
    now_millis, overlay, page, page_open, persistence, plugin, profile, team, terminal, ui_state,
    wake, workspace,
};
