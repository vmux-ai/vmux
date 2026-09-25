mod key;
mod media;
pub(crate) mod model;
mod prompt;
mod resume;
mod tab;
mod transcript;
mod workspace;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use vmux_chat::event::{ChatItem, ChatOpenPage, ChatTranscriptState};

pub struct AgentChatPagePlugin;

impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins((
            key::ChatKeyPlugin,
            media::ChatMediaPlugin,
            model::ChatModelPlugin,
            prompt::ChatPromptPlugin,
            resume::ChatResumePlugin,
            tab::ChatTabPlugin,
            transcript::ChatTranscriptPlugin,
            vmux_core::host::UiStatePlugin::<vmux_chat::state::ChatUiState>::default(),
            workspace::ChatWorkspacePlugin,
        ))
        .add_plugins(UiEventPlugin::<(ChatOpenPage,)>::default())
        .add_observer(on_chat_open_page);
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "sessions",
    title: "Sessions",
    title_message_id: None,
    replaces_command: None,
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

#[derive(Component)]
#[require(ChatUiStateUpdates, ChatTranscriptProjection)]
pub struct AgentChatView;

type ChatUiStateUpdates = vmux_core::host::UiState<vmux_chat::state::ChatUiState>;

#[derive(Component, Default)]
struct ChatTranscriptProjection {
    state: ChatTranscriptState,
    tail: Vec<ChatItem>,
    tail_start: u32,
}

#[derive(Component)]
pub(crate) struct ChatSynced;

fn on_chat_open_page(
    trigger: On<UiInput<ChatOpenPage>>,
    mut requests: MessageWriter<vmux_layout::stack::OpenRequest>,
) {
    let url = trigger.event().payload.url.clone();
    if url.is_empty() {
        return;
    }
    requests.write(vmux_layout::stack::OpenRequest { url: Some(url) });
}
