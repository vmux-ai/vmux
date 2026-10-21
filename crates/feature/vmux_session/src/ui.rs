#![allow(non_snake_case)]

use self::agent::ChatHeader;
use self::approval::ChatApprovalDock;
use self::composer::ChatDock;
use self::item::MD_CSS;
use self::keys::use_chat_keys;
use self::state::use_chat;
use self::transcript::ChatTranscript;
use dioxus::prelude::*;

mod activity;
mod format;

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "ChatUiState", "SessionCatalogUiState", "KeyClaimsUiState"],
    subtree,
    preserve_title,
    no_favicon,
    takes = vmux_ecs::PageMetadata
)]
pub(crate) struct ChatPage;

#[component]
pub fn Page() -> Element {
    let route = try_consume_context::<vmux_ecs::PageMetadata>()
        .and_then(|metadata| vmux_session::Route::parse(&metadata.url));
    match route {
        Some(vmux_session::Route::Manager) => rsx! { manager::SessionsManager {} },
        Some(vmux_session::Route::Session(id)) => rsx! { SessionDetail { session_id: id.0 } },
        None => rsx! { SessionDetail { session_id: String::new() } },
    }
}

#[component]
fn SessionDetail(session_id: String) -> Element {
    let chat = use_chat();
    let catalog = vmux_ui::hooks::use_ui_state::<crate::state::SessionCatalogUiState>();
    let keys = use_chat_keys(chat);
    use_context_provider(|| keys);
    let accent = chat.accent();
    let details = catalog.state.read().session_details(&session_id);
    let details_available = details.is_some();
    let mut details_open = use_signal(|| false);

    rsx! {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent.css};",
            tabindex: "-1",
            onkeydown: move |event| keys.on_root_keydown(event),
            style { dangerous_inner_html: MD_CSS }
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                section { class: "flex min-w-0 flex-1 flex-col",
                    ChatHeader {
                        chat,
                        details_available,
                        details_open: details_open(),
                        on_details_toggle: move |_| details_open.set(!details_open()),
                    }
                    ChatTranscript { chat }
                    ChatApprovalDock { chat }
                    ChatDock { chat }
                }
                if details_open() {
                    if let Some((session, repository, stages)) = details {
                        manager::SessionMetaPanel { session, repository, stages }
                    }
                }
            }
        }
    }
}

pub mod agent;
pub mod approval;
pub mod composer;
mod error;
mod item;
mod keys;
mod manager;
mod manager_controller;
mod manager_model;
mod scroll;
mod state;
mod transcript;
