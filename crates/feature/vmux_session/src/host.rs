use bevy_app::{App, Plugin};
#[cfg(all(host, not(target_os = "android")))]
use bevy_app::{PreStartup, Startup};
#[cfg(all(host, not(target_os = "android")))]
use bevy_ecs::prelude::{Commands, Name, World};
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::host_spawn::HostSpawnRoute;
#[cfg(all(host, not(target_os = "android")))]
use vmux_ecs::persistence::{WorkspaceRestoreValidator, WorkspaceStoreValidator};
#[cfg(host)]
use vmux_feature::AppFeatureExt;

#[cfg(host)]
pub use catalog::{ScopeProjectionRequest, SessionCatalogView, SessionManagerView};
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
    ChatSynced, ChatTranscriptProjection, PendingAgentChoice, SessionView, SessionViews,
    StartRequest, TranscriptPage, TranscriptTail, USER_CHOICE_REQUESTED,
};
pub use state::{ChatRuntime, PublishComposerEffect, RepublishUiChat};
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
mod resume;
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
        app.register_feature::<crate::Feature>().add_plugins((
            Self::MANIFEST.plugin(),
            catalog::CatalogPlugin,
            resume::Plugin,
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
    commands.spawn((
        Name::new("Session identity validator"),
        WorkspaceRestoreValidator::unique::<crate::Id<crate::Session>>("Session identity"),
    ));
    commands.spawn((
        Name::new("Session composition validator"),
        WorkspaceRestoreValidator::new("Session composition", validate_sessions),
    ));
}

#[cfg(all(host, not(target_os = "android")))]
fn validate_sessions(world: &mut World) -> Result<(), String> {
    let mut sessions = world.query_filtered::<(
        bevy_ecs::entity::Entity,
        Option<&crate::Id<crate::Session>>,
        Option<&Name>,
        Option<&vmux_ecs::Description>,
        Option<&vmux_ecs::Cwd>,
        Option<&vmux_ecs::CreatedAt>,
        Option<&vmux_ecs::LastActivatedAt>,
        Option<&crate::WorkflowStateRef>,
        Option<&crate::WorkflowStateChangedAt>,
    ), bevy_ecs::query::With<crate::Session>>();
    for (entity, id, name, description, cwd, created, activated, stage, changed) in
        sessions.iter(world)
    {
        if id.is_none()
            || name.is_none()
            || description.is_none()
            || cwd.is_none()
            || created.is_none()
            || activated.is_none()
            || stage.is_none()
            || changed.is_none()
        {
            return Err(format!("Session {entity} is partially initialized"));
        }
    }
    Ok(())
}

#[cfg(all(host, not(target_os = "android")))]
fn register_route(mut commands: Commands) {
    commands.spawn(HostSpawnRoute::subtree(vmux_api::VmuxRoute::SESSIONS_ROOT));
}
