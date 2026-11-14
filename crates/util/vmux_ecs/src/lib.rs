extern crate self as vmux_ecs;

pub use vmux_macro::types;

#[cfg(feature = "store")]
pub use vmux_macro::store;
pub use vmux_task::{
    AsyncTask, TaskApply, TaskFailure, TaskPlugin, TaskResult, TaskSet, TaskToken, TaskWaker,
};

#[cfg(all(host, feature = "host"))]
pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
#[cfg(feature = "jobs")]
pub use blocking::{BlockingCancellation, BlockingPool, BlockingRuntime, BlockingTask};
pub use component::{
    ActivateRequest, Active, ArchivedAt, Bookmark, BookmarkOrder, Collapsed, CollectionView,
    CreatedAt, Cwd, Description, EffectiveStartupUrl, EntityTarget, Folder, HostShell,
    JsonArguments, KeyboardOwner, LastActivatedAt, LastVisitedAt, Order, PagePresented, Pin, Ready,
    RegistrationOrder, Terminal, TransitionType, UnixMillis, Url, Uuid, Visit, VisitCount,
    VisitedUrl, WindowFullscreen, WindowFullscreenSet,
};
#[cfg(feature = "contracts")]
pub use component::{PendingPrompt, PendingPromptAttachments, ProcessAnchor};
pub use external_identity::{
    ExternalIdentity, ExternalIdentityIndex, ExternalIdentityIndexPlugin, ExternalIdentityIndexSet,
};
#[cfg(all(host, feature = "host"))]
pub use file_ui_state::{FileUiStateUpdates, FileUiStateWrite};
pub use identity::{
    IdentityIndex, IdentityIndexPlugin, IdentityIndexSet, IdentityIndexWriter, IdentityLookupError,
};
#[cfg(feature = "jobs")]
pub use jobs::{
    Job, JobOwner, JobPlugin, JobReady, JobRequest, JobResult, JobSet, JobWake, JobsPlugin,
};
#[cfg(all(host, feature = "host"))]
pub use launcher::{
    CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged,
    ContributedCommandChosen, HostsLauncher, InlineTransitionRequested, LauncherDismissRequest,
    RendersLauncherPanel, RestoreKeyboardToStack, StackInPaneChosen,
};
#[cfg(all(host, feature = "host"))]
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
pub use operation::{
    Operation, OperationBundle, OperationCompleted, OperationError, OperationKind, OperationPlugin,
    OperationState, OperationTarget, OperationTargetRevision, TargetedOperations,
};
#[cfg(all(host, feature = "host"))]
pub use overlay::{Overlay, OverlayShownInline, OverlayState, WindowOverlay};
#[cfg(feature = "contracts")]
pub use page_metadata::{PageIdentity, PageMetadata};
#[cfg(all(host, feature = "host"))]
pub use page_open::{
    CefPageAttachRequest, PageNavigation, PageOpenDeferred, PageOpenError, PageOpenHandled,
    PageOpenId, PageOpenRequest, PageOpenSet, PageOpenTarget, PageOpenTask, PageTransition,
};
pub use primitives::PrimitivesPlugin;
#[cfg(feature = "contracts")]
pub use process_id::ProcessId;
#[cfg(all(host, feature = "host"))]
pub use process_id::ProcessPresentationRequest;
pub use revision::{BaseRevision, Revision};
#[cfg(all(host, feature = "host"))]
pub use ui_state::{UiState, UiStateFlushSet, UiStatePlugin, UiStateWrite, UiStateWriter};
pub use workspace::WorkspaceKind;
#[cfg(all(host, feature = "host"))]
pub use workspace::{ComputeFocusSet, StackCommandSet, TabCommandSet};

#[cfg(all(host, feature = "host"))]
pub mod address;
#[cfg(all(host, feature = "host"))]
pub mod agent;
#[cfg(all(host, feature = "host"))]
mod archive;
#[cfg(feature = "jobs")]
pub mod blocking;
#[cfg(all(host, feature = "host"))]
pub mod browser;
#[cfg(all(host, feature = "host"))]
pub mod cli;
mod component;
#[cfg(feature = "contracts")]
pub mod event;
mod external_identity;
#[cfg(all(host, feature = "host"))]
mod file_ui_state;
#[cfg(all(host, feature = "host"))]
pub mod host_spawn;
mod identity;
#[cfg(feature = "jobs")]
pub mod jobs;
#[cfg(all(host, feature = "host"))]
pub mod launcher;
#[cfg(all(host, feature = "host"))]
pub mod manifest;
#[cfg(all(host, feature = "host"))]
pub mod notify;
mod operation;
#[cfg(all(host, feature = "host"))]
pub mod overlay;
#[cfg(all(host, feature = "host"))]
pub mod page;
#[cfg(all(host, feature = "host"))]
mod page_driver;
#[cfg(all(host, feature = "host"))]
pub mod page_location;
#[cfg(feature = "contracts")]
pub mod page_metadata;
#[cfg(all(host, feature = "host"))]
pub mod page_open;
mod primitives;
#[cfg(feature = "contracts")]
pub mod process_id;
#[cfg(all(host, feature = "host"))]
pub mod profile;
#[cfg(all(host, feature = "host"))]
mod registry;
pub mod revision;
pub mod scroll;
#[cfg(all(host, feature = "host"))]
pub mod search;
#[cfg(all(host, feature = "host"))]
pub mod service;
#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "contracts")]
pub mod team;
#[cfg(all(host, feature = "host"))]
pub mod terminal;
pub mod types;
#[cfg(all(host, feature = "host"))]
mod ui_state;
#[cfg(all(host, feature = "host"))]
pub mod wake;
pub mod workspace;
