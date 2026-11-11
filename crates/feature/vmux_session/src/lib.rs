#![allow(non_snake_case, clippy::type_complexity)]
#![cfg_attr(not(feature = "app"), allow(dead_code))]

extern crate self as vmux_session;

#[cfg(host)]
use bevy_app::{App, Plugin};

pub use catalog::{
    ArchiveState, CatalogSnapshot, ProviderSession, SessionAge, SessionSummary,
    SessionSummaryProjection, StageSummary,
};
#[cfg(host)]
pub use conversation::{
    ConversationAppendReceived, ConversationEvent, ConversationHistory, ConversationHistoryApplied,
    ConversationHistoryMoreRequest, ConversationPlugin, ConversationPrependReceived, Document,
    DocumentKind, EventIdentity, HistoryEntry, MaterializedEvent, Member, MessageContent,
    MessageDelivery, OperationCommitted, OperationReceived, SnapshotReceived, Transcript,
    TranscriptReadySet, Transcripts,
};
#[cfg(feature = "app")]
pub use host::SessionPlugin;
#[cfg(host)]
pub use model::ProviderSessionTombstones;
pub use model::{
    AgentFamily, AgentId, ArchiveRequest, Cleanup, CleanupRequest, CleanupTarget, CreateRequest,
    Created, DescriptionUpdateRequest, LocalTask, ProviderArchive, ProviderArchiveCompleted,
    ProviderArchiveRequest, ProviderArchiveWriteRequest, ProviderKind, ProviderSessionId,
    ProviderSessionKey, ProviderSessionRestoreRequest, ProviderSessionTombstone, ProviderStage,
    ProviderStageChangeRequest, RenameRequest, Session, SessionDuplicate, SourceUpdatedAt, Stage,
    StageChangeRequest, StageChangedAt, StageDefinition, StageId,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, Failure, FailureKind, RunState};
pub use session::{
    AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt, TitlePending,
    TranscriptLoading,
};
pub use title::ConversationTitle;
pub use vmux_api::session::SessionId;

#[cfg(all(host, feature = "app"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "app"))]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
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
#[cfg(host)]
pub mod history;
mod model;
#[cfg(host)]
pub mod mutation;
#[cfg(host)]
pub mod provider;
mod route;
mod title;

#[cfg(host)]
pub struct DomainPlugin;

#[cfg(host)]
impl Plugin for DomainPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<provider::Plugin>() {
            app.add_plugins(provider::Plugin);
        }
        app.add_plugins((
            model::EntityPlugin,
            model::MetadataPlugin,
            model::StagePlugin,
            catalog::CatalogPlugin,
        ));
        if !app.is_plugin_added::<history::Plugin>() {
            app.add_plugins(history::Plugin);
        }
    }
}
