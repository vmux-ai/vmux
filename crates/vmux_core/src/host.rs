//! Everything that needs a host to run on, rather than the browser bundle.
//!
//! One `host` gate for the lot, rather than an attribute on each declaration: these modules
//! are built on Bevy, which the wasm page bundle does not link. The crate's public paths are
//! unchanged — `lib.rs` re-exports this module's contents, so `vmux_core::page` still resolves
//! from outside and `crate::page` still resolves from within.

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

pub use archive::{
    ArchivedPage, ArchivedPagePosition, ArchivedTabPage, PageArchiveRequest, PaneStep, SplitAxis,
};
pub use host_spawn::{HostSpawnRegistry, register_host_spawn};
pub use launcher::{
    ContributedCommandChosen, FocusLauncherInput, HostsLauncher, InlineTransitionRequested,
    PendingLaunch, PendingStackAbandoned,
};
pub use notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
pub use overlay::{OverlayState, OverlayStateQuery, WindowOverlay};
pub use page_open::{
    CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenId, PageOpenRequest, PageOpenSet,
    PageOpenTarget, PageOpenTask, PendingPrompt, PendingPromptAttachments,
};
