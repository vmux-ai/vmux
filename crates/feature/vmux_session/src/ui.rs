#![allow(non_snake_case)]

use self::agent::DetailsToggle;
use self::approval::ChatApprovalDock;
use self::composer::ChatDock;
use self::item::MD_CSS;
use self::keys::use_chat_keys;
use self::state::use_chat;
use self::transcript::ChatTranscript;
use dioxus::prelude::*;

pub use manager::SessionCollection;

mod activity;
mod conversation;
mod format;

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "ChatUiState", "SessionCatalogUiState", "SessionDetailUiState", "KeyClaimsUiState"],
    subtree,
    preserve_title,
    no_favicon,
    document_url = "vmux://start/",
    takes = vmux_ecs::PageMetadata
)]
pub(crate) struct ChatPage;

#[component]
pub fn Page() -> Element {
    vmux_ui::hooks::use_ui_state::<crate::state::ChatUiState>();
    vmux_ui::hooks::use_ui_state::<crate::state::SessionCatalogUiState>();
    vmux_ui::hooks::use_ui_state::<crate::state::SessionDetailUiState>();
    let details_open = use_signal(|| false);
    let route = try_consume_context::<vmux_ecs::PageMetadata>()
        .and_then(|metadata| vmux_session::Route::parse(&metadata.url));
    match route {
        Some(vmux_session::Route::Manager) => rsx! { manager::SessionsManager {} },
        Some(vmux_session::Route::Session(id)) => {
            let session_id = id.0;
            rsx! { SessionDetail { key: "{session_id}", session_id, details_open } }
        }
        None => {
            let session_id = String::new();
            rsx! { SessionDetail { key: "{session_id}", session_id, details_open } }
        }
    }
}

#[component]
fn SessionDetail(session_id: String, mut details_open: Signal<bool>) -> Element {
    use_hook(vmux_ui::components::motion::Disclosure::install);
    let chat = use_chat(session_id.clone());
    let file_placement = item::FilePlacement::provide();
    let detail =
        vmux_ui::hooks::UiStateBinding::<crate::state::SessionDetailUiState>::use_context();
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

    rsx! {
        item::ToolFileMenu {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-background text-foreground outline-none",
            onresize: move |event: Event<ResizeData>| file_placement.resize(&event.data()),
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
                section { class: "relative flex min-w-0 flex-1 flex-col",
                    DetailsToggle {
                        details_available,
                        details_open: details_open(),
                        on_details_toggle: move |_| details_open.set(!details_open()),
                    }
                    ChatTranscript { chat }
                    div {
                        class: "relative isolate z-20 -mt-8 before:pointer-events-none before:absolute before:inset-x-0 before:-top-12 before:bottom-0 before:-z-10 before:bg-[linear-gradient(to_bottom,transparent,var(--background)_3rem)]",
                        ChatApprovalDock { chat }
                        ChatDock { chat }
                    }
                }
                div {
                    class: if details_open() {
                        "hidden h-full min-h-0 shrink-0 overflow-hidden opacity-100 transition-[width,opacity,transform] duration-200 ease-out motion-reduce:transition-none lg:flex lg:w-[19rem] lg:translate-x-0"
                    } else {
                        "pointer-events-none hidden h-full min-h-0 shrink-0 translate-x-3 overflow-hidden opacity-0 transition-[width,opacity,transform] duration-200 ease-out motion-reduce:transition-none lg:flex lg:w-0"
                    },
                    aria_hidden: (!details_open()).to_string(),
                    if let Some((session, repository, stages)) = details {
                        manager::SessionMetaPanel { session, repository, stages }
                    } else if details_open() {
                        div { class: "flex w-[19rem] flex-col gap-4 px-4 py-4",
                            vmux_ui::components::skeleton::Skeleton { class: "h-6 w-2/3" }
                            vmux_ui::components::skeleton::Skeleton { class: "h-20 w-full" }
                            vmux_ui::components::skeleton::Skeleton { class: "h-20 w-full" }
                        }
                    }
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
