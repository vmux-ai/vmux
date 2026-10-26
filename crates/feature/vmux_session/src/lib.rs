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
    ConversationAppendReceived, ConversationEvent, Document, DocumentKind, EventIdentity,
    MaterializedEvent, Member, MessageContent, MessageDelivery, OperationCommitted,
    OperationReceived, SnapshotReceived, Transcript, Transcripts,
};
#[cfg(feature = "app")]
pub use host::SessionPlugin;
#[cfg(host)]
pub use model::ProviderSessionTombstones;
pub use model::{
    AgentFamily, AgentId, ArchiveRequest, Cleanup, CleanupRequest, CleanupTarget, CreateRequest,
    Created, DescriptionUpdateRequest, ProviderArchive, ProviderArchiveCompleted,
    ProviderArchiveRequest, ProviderArchiveWriteRequest, ProviderSessionRestoreRequest,
    ProviderSessionTombstone, ProviderStage, ProviderStageChangeRequest, RenameRequest,
    SessionDuplicate, Stage, StageChangeRequest, StageChangedAt, StageDefinition, StageId,
    TerminalStage,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, RunState};
pub use session::{
    AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt, TitlePending,
    TranscriptLoading,
};
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
mod route;
mod title;

#[cfg(host)]
pub(crate) struct DomainPlugin;

#[cfg(host)]
impl Plugin for DomainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            model::EntityPlugin,
            model::MetadataPlugin,
            model::StagePlugin,
            catalog::CatalogPlugin,
            conversation::ConversationPlugin,
        ));
    }
}
