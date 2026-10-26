#![allow(non_snake_case)]

use self::agent::{ChatHeader, ChatHeaderCatalog};
use self::approval::ChatApprovalDock;
use self::composer::ChatDock;
use self::item::MD_CSS;
use self::keys::use_chat_keys;
use self::state::use_chat;
use self::transcript::ChatTranscript;
use dioxus::prelude::*;

pub use manager::SessionCollection;

mod activity;
mod format;

#[vmux_page::page(
    component = Page,
    states = [
        vmux_ui::theme::UiTheme,
        crate::state::UiChat,
        crate::state::UiSessionCatalog,
        crate::state::UiSessionDetail,
        vmux_api::input::UiKeyClaims,
    ],
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
        Some(vmux_session::Route::Session(id)) => {
            let session_id = id.into_string();
            rsx! { SessionDetail { key: "{session_id}", session_id } }
        }
        None => {
            let session_id = String::new();
            rsx! { SessionDetail { key: "{session_id}", session_id } }
        }
    }
}

#[component]
fn SessionDetail(session_id: String) -> Element {
    let chat = use_chat();
    let detail = vmux_ui::hooks::UiState::<crate::state::UiSessionDetail>::use_context();
    let header_catalog =
        detail.use_selector(|state| ChatHeaderCatalog::from_navigation(&state.navigation));
    let keys = use_chat_keys(chat);
    use_context_provider(|| keys);
    let accent = chat.accent();
    let details = {
        let state = detail.state.read();
        state
            .session
            .as_ref()
            .filter(|session| session.id == session_id)
            .map(|session| {
                (
                    session.clone(),
                    state.repository.clone(),
                    state.stages.clone(),
                )
            })
    };
    let details_available = details.is_some();
    let header_session = details.as_ref().map(|(session, _, _)| session.clone());
    let header_repository = details
        .as_ref()
        .and_then(|(_, repository, _)| repository.clone());
    let header_navigation = header_catalog
        .read()
        .navigation(header_repository.as_ref(), header_session.as_ref());
    let mut details_open = use_signal(|| false);

    rsx! {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent.css};",
            tabindex: "-1",
            onkeydown: move |event| {
                if matches!(event.key(), Key::ArrowUp | Key::PageUp | Key::Home) {
                    chat.transcript.note_scroll_input();
                }
                keys.on_root_keydown(event);
            },
            style { dangerous_inner_html: MD_CSS }
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                section { class: "flex min-w-0 flex-1 flex-col",
                    ChatHeader {
                        chat,
                        session: header_session,
                        repository: header_repository,
                        navigation: header_navigation,
                        details_available,
                        details_open: details_open(),
                        on_details_toggle: move |_| details_open.set(!details_open()),
                    }
                    ChatTranscript { chat }
                    ChatApprovalDock { chat }
                    ChatDock { chat }
                }
                if let Some((session, repository, stages)) = details {
                    div {
                        class: if details_open() {
                            "hidden h-full min-h-0 shrink-0 overflow-hidden opacity-100 transition-[width,opacity,transform] duration-200 ease-out motion-reduce:transition-none lg:flex lg:w-[19rem] lg:translate-x-0"
                        } else {
                            "pointer-events-none hidden h-full min-h-0 shrink-0 translate-x-3 overflow-hidden opacity-0 transition-[width,opacity,transform] duration-200 ease-out motion-reduce:transition-none lg:flex lg:w-0"
                        },
                        aria_hidden: (!details_open()).to_string(),
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
