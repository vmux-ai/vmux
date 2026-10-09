#![allow(non_snake_case, clippy::type_complexity)]
#![cfg_attr(not(feature = "app"), allow(dead_code))]

extern crate self as vmux_session;

#[cfg(host)]
use bevy_app::{App, Plugin};

pub use catalog::{CatalogSnapshot, SessionSummary, StageSummary};
#[cfg(host)]
pub use conversation::{
    ConversationEvent, Document, DocumentKind, EventIdentity, MaterializedEvent, Member,
    MessageContent, MessageDelivery, OperationCommitted, OperationReceived, SnapshotReceived,
    Transcript, Transcripts,
};
#[cfg(feature = "app")]
pub use host::SessionPlugin;
pub use model::{
    AgentId, Cleanup, CleanupRequest, CreateRequest, Created, DescriptionUpdateRequest, LocalTask,
    RenameRequest, Session, Stage, StageChangeRequest, StageChangedAt, StageDefinition, StageId,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, RunState};
pub use session::{AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt};
pub use title::ConversationTitle;
pub use vmux_api::conversation::SessionId;

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
