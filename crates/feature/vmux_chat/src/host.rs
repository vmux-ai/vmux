use bevy_app::{App, Plugin};
#[cfg(host)]
use vmux_ecs::manifest::FeaturePlugin;

#[cfg(host)]
pub use catalog::SessionManagerView;
pub use conversation::{Agents, Conversation, LiveTurn, Log, Reported, Submitted};
#[cfg(host)]
pub use handoff::ImportedConversation;
#[cfg(host)]
pub use media::ChatAttachmentHydrationRequest;
pub use model::{ChatModeStateChanged, ChatModelStateChanged, Models};
pub use prompt::{Attach, Attachments, Browsed, RemoveAttachment};
#[cfg(host)]
pub use session::{
    ChatAttachmentProjection, ChatBranchesProjection, ChatComposerContext, ChatHistoryQuery,
    ChatHistoryResult, ChatMediaProjection, ChatResumeProjection, ChatSnapshotProjection,
    ChatSynced, ChatTranscriptProjection, ChatView, PendingAgentChoice, TranscriptPage,
    TranscriptTail, USER_CHOICE_REQUESTED,
};
pub use state::{ChatRuntime, PublishComposerEffect, RepublishChatUiState};
#[cfg(host)]
pub use tool::ChatToolPlugin;

#[cfg(host)]
mod catalog;
#[cfg(host)]
mod command_bar;
#[cfg(host)]
mod composer;
mod conversation;
mod group;
#[cfg(host)]
mod handoff;
#[cfg(host)]
mod key;
#[cfg(host)]
mod media;
mod model;
mod presentation;
mod projection;
mod prompt;
#[cfg(host)]
mod session;
mod state;
#[cfg(host)]
mod tool;
#[cfg(host)]
mod transcript;

#[vmux_page::page]
pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            state::ChatUiStatePlugin,
            model::ChatModelPlugin,
            prompt::ChatPromptPlugin,
            conversation::ChatConversationPlugin,
        ));
        #[cfg(host)]
        app.add_plugins((
            FeaturePlugin::<crate::Feature>::default(),
            Self::MANIFEST.plugin(),
            catalog::CatalogPlugin,
            session::ChatHostPlugin,
            command_bar::Plugin,
            transcript::Plugin,
        ));
    }
}
