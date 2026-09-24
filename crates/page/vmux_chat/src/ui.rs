#![allow(non_snake_case)]

use self::agent::ChatHeader;
use self::approval::ChatApprovalDock;
use self::composer::ChatDock;
use self::keys::use_chat_keys;
use self::state::use_chat;
use self::transcript::ChatTranscript;
use crate::transcript::MD_CSS;
use dioxus::prelude::*;

#[component]
pub fn Page() -> Element {
    let chat = use_chat();
    let keys = use_chat_keys(chat);
    use_context_provider(|| keys);
    let accent = chat.accent();

    rsx! {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-zinc-100 text-foreground outline-none dark:bg-zinc-900",
            style: "--agent-accent:{accent.css};",
            tabindex: "-1",
            onkeydown: move |event| keys.on_root_keydown(event),
            style { dangerous_inner_html: MD_CSS }
            ChatHeader { chat }
            ChatTranscript { chat }
            ChatApprovalDock { chat }
            ChatDock { chat }
        }
    }
}

pub mod agent;
pub mod approval;
pub mod composer;
mod error;
mod keys;
mod scroll;
mod state;
mod transcript;
