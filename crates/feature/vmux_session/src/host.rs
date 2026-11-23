use bevy_app::{App, Plugin};
#[cfg(all(host, not(target_os = "android")))]
use bevy_app::{PreStartup, Startup};
#[cfg(all(host, not(target_os = "android")))]
use bevy_ecs::prelude::{Commands, Name};
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::host_spawn::HostSpawnRoute;
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::page::PrewarmPage;
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::store::{WorkspaceRestoreValidator, WorkspaceStoreValidator};
#[cfg(host)]
use vmux_feature::manifest::FeaturePlugin;

pub use SessionClientPlugin as SessionPlugin;
#[cfg(host)]
pub use catalog::{ScopeProjectionRequest, SessionCatalogView, SessionManagerView};
pub use conversation::{Agents, Conversation, LiveTurn, Log, Reported, Submitted};
#[cfg(host)]
pub use handoff::{HandoffEvent, HandoffMessages, ImportedConversation};
#[cfg(host)]
pub use media::ChatAttachmentHydrationRequest;
pub use model::{ChatModeStateChanged, ChatModelStateChanged, Models};
#[cfg(host)]
pub use onboarding::{ProvidersChanged, SetupTarget, SetupView};
#[cfg(host)]
pub use prompt::RecoverPrompt;
pub use prompt::{Attach, Attachments, Browsed, RemoveAttachment};
#[cfg(host)]
pub use session::{
    ChatAttachmentProjection, ChatBranchesProjection, ChatComposerContext, ChatHistoryQuery,
    ChatHistoryResult, ChatMediaProjection, ChatResumeProjection, ChatSnapshotProjection,
    ChatSynced, ChatTranscriptProjection, PendingAgentChoice, SessionView, SessionViews,
    StartRequest, TranscriptPage, TranscriptTail, USER_CHOICE_REQUESTED,
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
mod manager;
#[cfg(all(test, host))]
pub(crate) mod manager_fixture;
#[cfg(host)]
mod manager_operations;
#[cfg(host)]
mod media;
mod model;
#[cfg(host)]
mod onboarding;
mod presentation;
mod projection;
mod prompt;
#[cfg(host)]
mod resume;
#[cfg(host)]
mod session;
mod state;
#[cfg(host)]
mod tool;
#[cfg(host)]
mod transcript;
#[cfg(host)]
mod video;

#[vmux_page::page]
pub struct SessionClientPlugin;

impl Plugin for SessionClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            state::ChatUiStatePlugin,
            model::ChatModelPlugin,
            prompt::ChatPromptPlugin,
            conversation::ChatConversationPlugin,
        ));
        #[cfg(all(host, not(target_os = "android")))]
        {
            if !app.is_plugin_added::<crate::SessionHostPlugin>() {
                app.add_plugins(crate::SessionHostPlugin);
            }
            app.add_plugins(crate::model::MetadataClientPlugin)
                .add_systems(PreStartup, (spawn_store_validator, prewarm))
                .add_systems(Startup, register_route);
        }
        #[cfg(all(host, not(target_os = "android"), feature = "preferences"))]
        app.add_plugins(crate::preferences::PreferencesPlugin);
        #[cfg(host)]
        app.add_plugins((
            FeaturePlugin::<crate::Feature>::default(),
            Self::manifest().plugin(),
            catalog::CatalogPlugin,
            resume::ResumePlugin,
            session::ChatHostPlugin,
            command_bar::CommandBarPlugin,
            transcript::TranscriptPlugin,
            onboarding::OnboardingPlugin,
        ));
    }
}

#[cfg(all(host, not(target_os = "android")))]
fn prewarm(mut commands: Commands) {
    commands.spawn(PrewarmPage::exact(
        vmux_api::VmuxRoute::SESSIONS_ROOT,
        vmux_ui::i18n::translate("sessions-title"),
        1,
    ));
    commands.spawn(PrewarmPage::renderer(
        crate::Route::SETUP_URL,
        vmux_ui::i18n::translate("onboarding-setup"),
        1,
    ));
}

#[cfg(all(host, not(target_os = "android")))]
fn spawn_store_validator(mut commands: Commands) {
    commands.spawn(WorkspaceRestoreValidator::unique::<crate::SessionId>(
        "Session ID",
    ));
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
    commands.spawn(HostSpawnRoute::page(crate::Route::SETUP_URL));
}
