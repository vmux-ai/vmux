use bevy_app::{App, Plugin};

#[cfg(host)]
mod composer;
#[cfg(host)]
mod key;
#[cfg(host)]
mod media;
mod model;
mod prompt;
mod room;
#[cfg(host)]
mod session;
mod state;
#[cfg(host)]
mod tool;

#[cfg(host)]
pub use media::ChatAttachmentHydrationRequest;
pub use model::{ChatModeStateChanged, ChatModelStateChanged, Models};
pub use prompt::{Attach, Attachments, Browsed, RemoveAttachment};
pub use room::{Agents, Conversation, LiveTurn, Log, Reported, Submitted};
#[cfg(host)]
pub use session::{
    ChatAttachmentProjection, ChatBranchesProjection, ChatComposerContext, ChatHistoryQuery,
    ChatHistoryResult, ChatMediaProjection, ChatPlugin, ChatResumeProjection,
    ChatSnapshotProjection, ChatSynced, ChatTranscriptProjection, ChatView, PendingAgentChoice,
    TranscriptPage, TranscriptTail, USER_CHOICE_REQUESTED,
};
pub use state::{ChatRuntime, PublishComposerEffect, RepublishChatUiState};
#[cfg(host)]
pub use tool::ChatToolPlugin;

pub struct ChatStatePlugin;

impl Plugin for ChatStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            model::ChatModelPlugin,
            prompt::ChatPromptPlugin,
            room::ChatRoomPlugin,
        ));
    }
}
