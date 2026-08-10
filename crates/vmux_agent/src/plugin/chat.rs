//! The desktop half of the chat page: owning the session's ECS state and moving it between the
//! daemon and the webview.
//!
//! Gated as a whole rather than item by item — a hundred attributes down one file said nothing
//! that one on the module does not. The rendered counterpart is `vmux_chat::page`, which this
//! reaches only through the payloads in `vmux_chat::event`; everything here that is about
//! driving an agent — sessions, strategies, run state — stays on this side of that line.

mod key;
mod media;
mod model;
mod prompt;
mod resume;
mod transcript;
mod workspace;

use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

use vmux_chat::event::ChatOpenPage;

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
            transcript::ChatTranscriptPlugin,
            workspace::ChatWorkspacePlugin,
        ))
        .add_plugins(BinEventEmitterPlugin::<(ChatOpenPage,)>::for_hosts(&[
            "agent", "start",
        ]))
        .add_observer(on_chat_open_page);
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

/// Marks a chat-page webview (ACP or Page agent) so the ready→resync path can find it cheaply.
///
/// Lives here rather than in a slice because every slice that emits to the page queries it, and
/// the stack machinery outside this module spawns it.
#[derive(Component)]
pub struct AgentChatView;

/// Set once the current snapshot has been pushed to a ready chat webview; cleared when the page
/// (re)signals ready (mount or Cmd+R reload) so the transcript is re-pushed instead of blanking.
#[derive(Component)]
pub(crate) struct ChatSynced;

/// Open a vmux page URL in a new stack (the error card's "change version" action → `vmux://agents`).
fn on_chat_open_page(
    trigger: On<BinReceive<ChatOpenPage>>,
    mut commands: MessageWriter<vmux_command::AppCommand>,
) {
    let url = trigger.event().payload.url.clone();
    if url.is_empty() {
        return;
    }
    commands.write(vmux_command::AppCommand::Browser(
        vmux_command::BrowserCommand::Open(vmux_command::open::OpenCommand::InNewStack {
            url: Some(url),
        }),
    ));
}
