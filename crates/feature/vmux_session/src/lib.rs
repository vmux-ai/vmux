#![allow(clippy::type_complexity)]

use bevy_app::{App, Plugin};

pub use catalog::{CatalogSnapshot, SessionSummary, StageSummary};
pub use conversation::{
    ConversationEvent, ConversationOperationCommitted, ConversationOperationReceived,
    ConversationSnapshotReceived, Document, DocumentKind, EventIdentity, MaterializedEvent, Member,
    MessageContent, MessageDelivery, Transcript, Transcripts,
};
pub use model::{
    AcpSessionId, AgentId, LocalTask, Session, SessionCleanupRequest, SessionCreateRequest,
    SessionDescriptionUpdateRequest, SessionMutationSet, SessionRenameRequest,
    SessionStageChangeRequest, Stage, StageChangedAt, StageDefinition, StageId,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, RunState};
pub use session::{AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt};
pub use title::ConversationTitle;
pub use vmux_ecs::agent::SessionId;

mod catalog;
mod conversation;
mod model;
mod route;
pub mod run_state;
pub mod session;
mod title;

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((model::ModelPlugin, conversation::ConversationPlugin));
    }
}
