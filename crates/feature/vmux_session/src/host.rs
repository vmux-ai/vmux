use bevy_app::{App, Plugin};
#[cfg(all(host, not(target_os = "android")))]
use bevy_app::{PreStartup, Startup};
#[cfg(all(host, not(target_os = "android")))]
use bevy_ecs::prelude::{Commands, Name};
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::host_spawn::HostSpawnRoute;
#[cfg(host)]
use vmux_ecs::manifest::FeaturePlugin;
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::persistence::WorkspaceStoreValidator;

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
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            state::ChatUiStatePlugin,
            model::ChatModelPlugin,
            prompt::ChatPromptPlugin,
            conversation::ChatConversationPlugin,
        ));
        #[cfg(all(host, not(target_os = "android")))]
        app.add_plugins(crate::DomainPlugin)
            .add_systems(PreStartup, spawn_store_validator)
            .add_systems(Startup, register_route);
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

#[cfg(all(host, not(target_os = "android")))]
fn spawn_store_validator(mut commands: Commands) {
    commands.spawn((
        Name::new("Session workspace-store validator"),
        WorkspaceStoreValidator {
            name: "Session URL",
            rejects: crate::Route::rejects_persisted_store,
        },
    ));
}

#[cfg(all(host, not(target_os = "android")))]
fn register_route(mut commands: Commands) {
    commands.spawn(HostSpawnRoute::subtree(vmux_api::VmuxRoute::SESSIONS_ROOT));
}
