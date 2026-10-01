#![allow(clippy::type_complexity)]

use bevy_app::{App, Plugin};

pub use acp::AcpSession;
pub use run_state::{AgentRunState, AgentTurnMeta};
pub use session::{
    AgentApprovalPolicy, AgentConversationTitle, AgentMessageTimes, AgentMessages, PromptQueue,
    QueuedPrompt,
};
pub use title::ConversationTitle;

pub mod acp;
pub mod room;
pub mod run_state;
pub mod session;
mod title;

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(room::RoomPlugin);
    }
}
