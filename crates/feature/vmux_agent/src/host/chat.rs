use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use vmux_chat::event::ChatOpenPage;

pub struct AgentChatPagePlugin;

impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PAGE_MANIFEST.plugin())
            .add_plugins((
                vmux_chat::ChatKeyPlugin,
                vmux_chat::ChatMediaPlugin,
                vmux_chat::composer::ChatComposerPlugin,
                super::model::ChatModelPlugin,
                super::composer::AgentChatComposerPlugin,
                super::prompt::ChatPromptPlugin,
                super::resume::ChatResumePlugin,
                super::tab::ChatTabPlugin,
                super::transcript::ChatTranscriptPlugin,
                vmux_core::host::UiStatePlugin::<vmux_chat::state::ChatUiState>::default(),
            ))
            .add_plugins(UiEventPlugin::<(ChatOpenPage,)>::default())
            .add_observer(on_chat_open_page);
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    url: "vmux://sessions/",
    asset_host: "sessions",
    owns_subtree: true,
    title: "Sessions",
    title_message_id: None,
    replaces_command: None,
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

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
