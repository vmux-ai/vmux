#![allow(non_snake_case, clippy::type_complexity)]
#![cfg_attr(not(feature = "app"), allow(dead_code))]

extern crate self as vmux_session;

#[cfg(host)]
use bevy_app::{App, Plugin};

pub use catalog::{
    ArchiveState, CatalogSnapshot, ProviderCatalog, ProviderSession, SessionAge, SessionSummary,
    SessionSummaryProjection, StageSummary,
};
#[cfg(host)]
pub use conversation::{
    ActorRef, ActorTarget, AuthoredEvents, ConversationAppendReceived, ConversationEvent, Document,
    DocumentKind, MaterializedEvent, Member, MessageContent, MessageDelivery, OperationCommitted,
    OperationReceived, Replies, ReplyTo, ReplyToId, Sequence, SnapshotReceived, Transcript,
    Transcripts,
};
#[cfg(feature = "app")]
pub use host::SessionPlugin;
#[cfg(host)]
pub use model::ProviderSessionTombstones;
pub use model::{
    Agent, AgentFamily, AgentRef, ArchiveRequest, Cleanup, CleanupRequest, CleanupTarget,
    CreateRequest, Created, DescriptionUpdateRequest, ProviderArchive, ProviderArchiveCompleted,
    ProviderArchiveRequest, ProviderArchiveWriteRequest, ProviderSessionRestoreRequest,
    ProviderSessionTombstone, ProviderWorkflowState, ProviderWorkflowStateChangeRequest,
    RenameRequest, SessionDuplicate, TerminalWorkflowState, WorkflowState,
    WorkflowStateChangeRequest, WorkflowStateChangedAt, WorkflowStateRef, WorkflowStateSources,
    WorkflowStateTarget,
};
#[cfg(host)]
pub use mutation::{
    CreatedMutation, SessionMutation, SessionMutationCommitSet, SessionMutationCommitted,
    SessionMutationReceiveSet, SessionMutationRejected, SessionMutationRejection,
    SessionMutationRequest,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, RunState};
pub use session::{
    AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt, TitlePending,
    TranscriptLoading,
};
pub use taxonomy::{
    AssignedSessions, AssigneeRef, AssigneeTarget, Cycle, CycleBundle, CycleEndsAt, CycleRef,
    CycleSessions, CycleStartsAt, CycleTarget, DueAt, Estimate, Label, LabelAssignment,
    LabelAssignments, LabelBundle, LabelColor, LabelRef, LabelTarget, Priority,
};
#[cfg(host)]
pub use taxonomy::{CycleSaveRequest, LabelSaveRequest, TaxonomyUpdateRequest};
pub use title::ConversationTitle;
pub use vmux_api::{Id, session::Session};

#[cfg(all(host, feature = "app"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "app"))]
impl vmux_feature::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(feature = "app")]
pub mod activity;
#[cfg(feature = "app")]
pub mod event;
#[cfg(feature = "app")]
pub mod host;
pub mod run_state;
#[cfg(feature = "app")]
pub mod selector;
pub mod session;
#[cfg(feature = "app")]
pub mod state;
#[cfg(feature = "app")]
pub mod tab;

#[cfg(all(ui, feature = "app"))]
pub mod ui;

mod catalog;
#[cfg(host)]
mod conversation;
mod model;
#[cfg(host)]
mod mutation;
mod route;
mod taxonomy;
mod title;

#[cfg(host)]
pub struct SessionDomainPlugin;

#[cfg(host)]
impl Plugin for SessionDomainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            model::EntityPlugin,
            model::MetadataPlugin,
            model::WorkflowStatePlugin,
            catalog::CatalogPlugin,
            conversation::ConversationPlugin,
            taxonomy::TaxonomyPlugin,
        ));
    }
}
