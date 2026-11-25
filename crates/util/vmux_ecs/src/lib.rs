extern crate self as vmux_ecs;

pub use vmux_macro::ecs_channels as channels;
pub use vmux_macro::types;

pub use vmux_channel as channel;
#[cfg(feature = "store")]
pub use vmux_macro::ecs_store as store;
pub use vmux_task::{
    AsyncTask, IoTask, IoTaskList, IoTaskPlugin, IoTaskSet, TaskApply, TaskError, TaskFailure,
    TaskPlugin, TaskResult, TaskSet, TaskToken, TaskWake, TaskWaker,
};
#[cfg(feature = "host")]
pub use vmux_task::{TokioTask, TokioTaskOutcome};
pub use vmux_time as time;
#[cfg(all(host, feature = "watch"))]
pub use vmux_watch as watch;

#[cfg(all(host, feature = "host"))]
pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
#[cfg(feature = "jobs")]
pub use blocking::{BlockingCancellation, BlockingPool, BlockingRuntime, BlockingTask};
pub use component::{
    Active, Bookmark, BookmarkOrder, Collapsed, CollectionView, Cwd, Description,
    EffectiveStartupUrl, EntityTarget, Folder, HostShell, JsonArguments, KeyboardOwner,
    PagePresented, Pin, RegistrationOrder, Terminal, TransitionType, Url, Uuid, Visit, VisitCount,
    VisitedUrl, WindowFullscreen, WindowFullscreenSet,
};
#[cfg(feature = "contracts")]
pub use component::{PendingPrompt, PendingPromptAttachments, ProcessAnchor};
pub use dirty::{Dirty, DirtyPlugin, DirtySet};
pub use external_identity::{
    ExternalIdentity, ExternalIdentityIndex, ExternalIdentityIndexPlugin, ExternalIdentityIndexSet,
};
#[cfg(all(host, feature = "host"))]
pub use file_ui_state::{FileUiStateUpdates, FileUiStateWrite};
pub use generation::{
    AtGeneration, Generation, GenerationPlugin, GenerationSet, GlobalSequence, SequenceCounter,
    Stale,
};
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
pub use lease::{LeaseGranted, LeaseMode, LeasePlugin, LeaseRequest, LeaseSet};
#[cfg(all(host, feature = "host"))]
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
pub use operation::{
    Operation, OperationAdmitted, OperationBundle, OperationCompleted, OperationError,
    OperationKind, OperationPlugin, OperationPolicy, OperationRetained, OperationRetry,
    OperationRetryRequested, OperationRetryWaiting, OperationSet, OperationState, OperationTarget,
    OperationTargetRevision, OperationTimeout, TargetedOperations,
};
pub use order::{Order, OrderPlugin, OrderPosition, OrderSet, ReorderRequest, Reordered};
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
pub use profile_identity::{ProfileId, ProfileLabel};
pub use rename::{Icon, RenamePlugin, RenameRequest, Renamed};
pub use resource_path::{ContentDigest, FilePath};
pub use revision::{BaseRevision, Revision};
pub use selection::{SelectRequest, Selected, SelectionPlugin, SelectionSet};
pub use shutdown::{
    Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownPending, ShutdownPhase, ShutdownPlugin,
    ShutdownRequest, ShutdownSet, ShutdownState, ShutdownTimeout,
};
pub use status::{Busy, Failure, Loading, Progress, Ready, Retired, StatusPlugin};
#[cfg(feature = "store")]
pub use store::{
    AutoSave, AutoSavePaused, AutoSavePlugin, AutoSavePolicy, AutoSaveSet, SchemaPlugin, StorePath,
    StoreWriteRequest, StoreWriteResult,
};
pub use timestamp::{
    ActivateRequest, ArchiveRequest, ArchivedAt, CreatedAt, ExpiresAt, LastActivatedAt,
    LastVisitedAt, TimestampPlugin, Touch, UnarchiveRequest, UnixMillis, UpdatedAt,
};
pub use toggle::{Disabled, Toggle, TogglePlugin, Toggled, TogglesPlugin};
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
#[cfg(feature = "cli")]
pub mod cli;
mod component;
mod dirty;
#[cfg(feature = "contracts")]
pub mod event;
mod external_identity;
#[cfg(all(host, feature = "host"))]
mod file_ui_state;
mod generation;
#[cfg(all(host, feature = "host"))]
pub mod host_spawn;
mod identity;
#[cfg(feature = "jobs")]
pub mod jobs;
#[cfg(all(host, feature = "host"))]
pub mod launcher;
mod lease;
#[cfg(all(host, feature = "host"))]
pub mod notify;
mod operation;
mod order;
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
mod profile_identity;
#[cfg(feature = "records")]
pub mod record;
mod rename;
mod resource_path;
pub mod revision;
pub mod scroll;
#[cfg(all(host, feature = "host"))]
pub mod search;
mod selection;
#[cfg(all(host, feature = "host"))]
pub mod service;
mod shutdown;
mod status;
#[cfg(feature = "store")]
pub mod store {
    pub use crate::workspace_store::{
        PageRestore, StoreAppExt, StoreDirty, WorkspaceCommitCompleted, WorkspaceCommitRequest,
        WorkspaceRestore, WorkspaceRestoreValidator, WorkspaceSaveRequest, WorkspaceStore,
        WorkspaceStoreValidator, WorkspaceStoreValidators, WorkspaceStored,
    };
    pub use vmux_store::{
        App, AutoSave, AutoSavePaused, AutoSavePlugin, AutoSavePolicy, AutoSaveSet, Dirty, Phase,
        RestoreValidator, Save, Schema, SchemaPlugin, State, StoreDomain, StorePath,
        StoreValidator, StoreValidators, StoreWriteRequest, StoreWriteResult, TypeRegistry, Types,
        With, WorldFilter,
    };
}
#[cfg(feature = "contracts")]
pub mod team;
#[cfg(all(host, feature = "host"))]
pub mod terminal;
mod timestamp;
mod toggle;
pub mod types;
#[cfg(all(host, feature = "host"))]
mod ui_state;
#[cfg(all(host, feature = "host"))]
pub mod wake;
pub mod workspace;
#[cfg(feature = "store")]
mod workspace_store;
