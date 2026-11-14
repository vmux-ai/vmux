#![allow(non_snake_case)]

use self::agent::DetailsToggle;
use self::approval::ChatApprovalDock;
use self::composer::ChatDock;
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
    states = ["PageAddressUiState", "ThemeUiState", "ChatUiState", "SessionCatalogUiState", "SessionDetailUiState", "OnboardingUiState", "KeyEntryUiState", "KeyClaimsUiState"],
    subtree,
    dom_group = "agent-conversation",
    preserve_title,
    no_favicon,
    document_url = "vmux://start/",
    takes = vmux_ecs::PageMetadata
)]
pub(crate) struct ChatPage;

#[vmux_page::page(
    file = "src/feature.ron",
    page = "agent-onboarding",
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "ChatUiState", "SessionCatalogUiState", "SessionDetailUiState", "OnboardingUiState", "KeyEntryUiState", "KeyClaimsUiState"],
    document_url = "vmux://start/",
    dom_group = "agent-conversation",
    preserve_title,
    no_favicon,
    takes = vmux_ecs::PageMetadata
)]
pub(crate) struct AgentOnboardingPage;

#[component]
pub fn Page() -> Element {
    vmux_ui::hooks::use_theme();
    vmux_ui::hooks::use_ui_state::<crate::state::ChatUiState>();
    vmux_ui::hooks::use_ui_state::<crate::state::SessionCatalogUiState>();
    vmux_ui::hooks::use_ui_state::<crate::state::SessionDetailUiState>();
    vmux_ui::hooks::use_ui_state::<crate::onboarding::OnboardingUiState>();
    vmux_ui::hooks::use_ui_state::<crate::onboarding::KeyEntryUiState>();
    let details_open = use_signal(|| false);
    let onboarding =
        vmux_ui::hooks::UiStateBinding::<crate::onboarding::OnboardingUiState>::use_context();
    let route = try_consume_context::<vmux_ecs::PageMetadata>()
        .and_then(|metadata| vmux_session::Route::parse(&metadata.url));
    match route {
        Some(vmux_session::Route::Manager) => rsx! { manager::SessionsManager {} },
        Some(vmux_session::Route::Provider { kind, sid }) => rsx! { PendingSession { kind, sid } },
        Some(vmux_session::Route::Setup) => {
            let session_id = onboarding.state.read().session_id.clone();
            if session_id.is_empty() {
                rsx! { onboarding::Onboarding {} }
            } else {
                rsx! { SessionDetail { key: "{session_id}", session_id, details_open } }
            }
        }
        Some(vmux_session::Route::Session(id)) => {
            let session_id = id.0;
            rsx! { SessionDetail { key: "{session_id}", session_id, details_open } }
        }
        None => rsx! { SessionDetail { session_id: String::new(), details_open } },
    }
}

#[component]
fn PendingSession(kind: String, sid: String) -> Element {
    let detail =
        vmux_ui::hooks::UiStateBinding::<crate::state::SessionDetailUiState>::use_context();
    let failed = detail
        .state
        .read()
        .pending_provider
        .as_ref()
        .is_some_and(|provider| provider.kind == kind && provider.sid == sid && provider.failed);
    rsx! {
        main { class: "session-chat-page flex h-dvh flex-col overflow-hidden bg-background text-foreground",
            div { class: "mx-auto w-full max-w-3xl px-3 pt-3 sm:px-4 md:px-6",
                if failed {
                    vmux_ui::components::notice::Notice {
                        message: vmux_ui::i18n::translate("sessions-history-unavailable"),
                        kind: vmux_ui::components::notice::NoticeKind::Warning,
                        actions: rsx! { vmux_ui::components::button::Button {
                            onclick: move |_| vmux_ui::navigation::PageNavigation::replace(vmux_session::Route::Manager.url()),
                            {vmux_ui::i18n::translate("sessions-title")}
                        } },
                    }
                } else {
                    transcript::TranscriptSkeleton {}
                }
            }
        }
    }
}

#[component]
fn SessionDetail(session_id: String, mut details_open: Signal<bool>) -> Element {
    let chat = use_chat(session_id.clone());
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
        surface::Page {
            accent: accent.css,
            setup: chat.setup_pending(),
            on_keydown: move |event: KeyboardEvent| {
                if matches!(event.key(), Key::ArrowUp | Key::PageUp | Key::Home) {
                    chat.transcript.note_scroll_input();
                }
                keys.on_root_keydown(event);
            },
            if !chat.setup_pending() { DetailsToggle {
                details_available,
                details_open: details_open(),
                on_details_toggle: move |_| details_open.set(!details_open()),
            } }
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                section { class: "relative flex min-w-0 flex-1 flex-col",
                    ChatTranscript { chat, reserve_details_toggle: !details_open() }
                    div {
                        class: "relative isolate z-20 -mt-8 before:pointer-events-none before:absolute before:inset-x-0 before:-top-12 before:bottom-0 before:-z-10 before:bg-[linear-gradient(to_bottom,transparent,var(--background)_3rem)]",
                        ChatApprovalDock { chat }
                        ChatDock { chat }
                    }
                }
                div {
                    class: if details_open() {
                        "visible absolute inset-y-0 right-0 z-40 flex h-full min-h-0 w-[min(19rem,100%)] shrink-0 overflow-hidden bg-background opacity-100 transition-[width,opacity,transform,visibility] duration-200 ease-out motion-reduce:transition-none lg:static lg:w-[19rem] lg:translate-x-0"
                    } else {
                        "invisible pointer-events-none absolute inset-y-0 right-0 z-40 flex h-full min-h-0 w-[min(19rem,100%)] shrink-0 translate-x-full overflow-hidden bg-background opacity-0 transition-[width,opacity,transform,visibility] duration-200 ease-out motion-reduce:transition-none lg:static lg:w-0 lg:translate-x-3"
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

pub mod agent;
pub mod approval;
mod authentication;
pub mod composer;
mod error;
mod item;
mod keys;
mod manager;
mod manager_controller;
mod manager_model;
mod onboarding;
mod scroll;
mod setup;
mod state;
mod surface;
mod transcript;

#[derive(Clone, Copy)]
pub(super) struct FilePlacement(Signal<f64>);

impl FilePlacement {
    pub(super) fn provide() -> Self {
        let placement = Self(use_signal(|| 0.0));
        use_context_provider(|| placement)
    }

    pub(super) fn resize(mut self, data: &ResizeData) {
        if let Ok(size) = data.get_content_box_size() {
            self.0.set(size.width);
        }
    }
}

#[cfg(test)]
mod page_routing_tests {
    #[test]
    fn setup_and_connected_conversations_preserve_the_same_document_and_dom() {
        let setup = super::AgentOnboardingPage::PAGE;
        let session = super::ChatPage::PAGE;
        assert!(setup.preserves_dom_for(&session));
        assert!(session.preserves_dom_for(&setup));
    }
}
