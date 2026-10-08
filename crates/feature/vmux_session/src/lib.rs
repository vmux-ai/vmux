#![allow(clippy::type_complexity)]

#[cfg(host)]
use bevy_app::{App, Plugin};

pub use catalog::{CatalogSnapshot, SessionSummary, StageSummary};
#[cfg(host)]
pub use conversation::{
    ConversationEvent, Document, DocumentKind, EventIdentity, MaterializedEvent, Member,
    MessageContent, MessageDelivery, OperationCommitted, OperationReceived, SnapshotReceived,
    Transcript, Transcripts,
};
pub use model::{
    AcpSessionId, AgentId, Cleanup, CleanupRequest, CreateRequest, Created,
    DescriptionUpdateRequest, LocalTask, RenameRequest, Session, Stage, StageChangeRequest,
    StageChangedAt, StageDefinition, StageId,
};
pub use route::Route;
pub use run_state::{AgentTurnMeta, RunState};
pub use session::{AgentConversationTitle, ApprovalPolicy, PromptQueue, QueuedPrompt};
pub use title::ConversationTitle;
pub use vmux_api::conversation::SessionId;

mod catalog;
#[cfg(host)]
mod conversation;
mod model;
mod route;
pub mod run_state;
pub mod session;
mod title;

#[cfg(host)]
pub struct SessionPlugin;

#[cfg(host)]
impl Plugin for SessionPlugin {
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
