use super::state::Chat;
use std::collections::HashSet;

use dioxus::prelude::*;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar, Identicon};
use vmux_ui::components::breadcrumb::{
    BreadcrumbItem, BreadcrumbOption, BreadcrumbSelector, BreadcrumbSeparator, Breadcrumbs,
};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::components::page::PageHeader;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::navigation::PageNavigation;

use crate::event::SessionRepository;
use crate::state::SessionCatalogUiState;
use crate::{Route, SessionId, SessionSummary};

#[derive(Clone, Default, PartialEq)]
pub(super) struct ChatHeaderCatalog {
    projects: Vec<BreadcrumbOption>,
    sessions: Vec<ChatHeaderSession>,
}

#[derive(Clone, PartialEq)]
struct ChatHeaderSession {
    project_url: String,
    option: BreadcrumbOption,
}

impl ChatHeaderCatalog {
    pub(super) fn from_catalog(catalog: &SessionCatalogUiState) -> Self {
        let mut project_urls = HashSet::new();
        let mut projects = Vec::new();
        for repository in &catalog.repositories.sessions {
            if repository.project.is_empty()
                || repository.project_url.is_empty()
                || !project_urls.insert(repository.project_url.clone())
            {
                continue;
            }
            projects.push(BreadcrumbOption {
                label: repository.project.clone(),
                value: repository.project_url.clone(),
            });
        }
        projects.sort_by(|left, right| left.label.cmp(&right.label));

        let now = vmux_ecs::UnixMillis::now().0;
        let mut sessions = Vec::new();
        for session in &catalog.catalog.sessions {
            if session.archived_at > 0 {
                continue;
            }
            let presentation =
                vmux_session::SessionSummaryProjection::new(session, &catalog.catalog.stages, now);
            let project_url = catalog
                .repositories
                .get(&session.id)
                .map(|repository| repository.project_url)
                .unwrap_or_default();
            sessions.push(ChatHeaderSession {
                project_url,
                option: BreadcrumbOption {
                    label: presentation.title,
                    value: Route::Session(SessionId(session.id.clone())).url(),
                },
            });
        }
        Self { projects, sessions }
    }

    pub(super) fn navigation(
        &self,
        repository: Option<&SessionRepository>,
    ) -> ChatHeaderNavigation {
        let current_project = repository.map(|repository| repository.project_url.as_str());
        let sessions = self
            .sessions
            .iter()
            .filter(|session| current_project.is_none_or(|project| session.project_url == project))
            .map(|session| session.option.clone())
            .collect();
        ChatHeaderNavigation {
            projects: self.projects.clone(),
            sessions,
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct ChatHeaderNavigation {
    projects: Vec<BreadcrumbOption>,
    sessions: Vec<BreadcrumbOption>,
}

#[component]
pub(super) fn ChatHeader(
    chat: Chat,
    session: Option<SessionSummary>,
    repository: Option<SessionRepository>,
    navigation: ChatHeaderNavigation,
    details_available: bool,
    details_open: bool,
    on_details_toggle: EventHandler<MouseEvent>,
) -> Element {
    let title = chat.title();
    let user_name = chat.user.name();
    let user_color = chat.user.color();
    let project_name = repository
        .as_ref()
        .map(|repository| repository.project.clone())
        .filter(|project| !project.is_empty())
        .unwrap_or_else(|| translate("layout-projects"));
    let project_seed = repository
        .as_ref()
        .map(|repository| repository.project_url.clone())
        .filter(|seed| !seed.is_empty())
        .or_else(|| session.as_ref().map(|session| session.cwd.clone()))
        .unwrap_or_else(|| project_name.clone());
    let session_seed = session
        .as_ref()
        .map(|session| session.id.clone())
        .unwrap_or_else(|| title.clone());
    rsx! {
        PageHeader {
            class: "session-chat-header relative z-40 flex h-14 shrink-0 items-center bg-transparent px-2.5".to_string(),
            content_class: "flex min-w-0 flex-1 items-center gap-2".to_string(),
            Breadcrumbs { class: "min-w-0 flex-1".to_string(),
                BreadcrumbSelector {
                    label: project_name.clone(),
                    class: "max-w-[min(16rem,30vw)] shrink-0".to_string(),
                    options: navigation.projects,
                    leading: rsx! {
                        Identicon {
                            seed: project_seed,
                            alt: project_name.clone(),
                            class: "size-5 shrink-0 rounded-md".to_string(),
                        }
                    },
                    onselect: move |url| PageNavigation::replace(url),
                }
                BreadcrumbSeparator {}
                BreadcrumbItem {
                    label: translate("sessions-title"),
                    onclick: move |_| PageNavigation::replace("vmux://sessions/"),
                }
                BreadcrumbSeparator {}
                BreadcrumbSelector {
                    label: title.clone(),
                    class: "min-w-0 flex-1".to_string(),
                    active: true,
                    options: navigation.sessions,
                    leading: rsx! {
                        Identicon {
                            seed: session_seed,
                            alt: title.clone(),
                            class: "size-5 shrink-0 rounded-md".to_string(),
                        }
                    },
                    onselect: move |url| PageNavigation::replace(url),
                }
            }
            StatusDot { status: chat.status(), size_class: "h-2 w-2" }
            div { class: "relative h-7 w-10 shrink-0",
                Avatar {
                    src: None,
                    seed: user_name.clone(),
                    background: user_color,
                    alt: user_name.clone(),
                    class: "absolute left-0 top-0 size-7 border-2 border-background text-[9px]".to_string(),
                }
                AgentAvatar { chat, size_class: "absolute left-4 top-0 size-7 border-2 border-background text-[9px]" }
            }
            if details_available {
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    class: if details_open {
                        "hidden bg-foreground/[0.08] text-foreground lg:inline-flex"
                    } else {
                        "hidden lg:inline-flex"
                    },
                    aria_label: translate("agent-details"),
                    title: translate("agent-details"),
                    onclick: move |event| on_details_toggle.call(event),
                    LineIconView { icon: LineIcon::Layout, class: "size-4" }
                }
            }
        }
    }
}

#[component]
fn AgentAvatar(chat: Chat, size_class: String) -> Element {
    let agent = chat.agent();
    if agent.is_empty() {
        return rsx! {
            span { class: "{size_class} invisible" }
        };
    }
    let accent = chat.identity.accent();
    let icon = chat.identity.icon();
    let src = (!icon.is_empty()).then_some(icon);
    let fallback = if accent.is_empty() {
        "#6366f1"
    } else {
        &accent
    };
    rsx! {
        AgentThumbnail {
            src,
            seed: agent,
            background: fallback,
            alt: chat.header_name(),
            class: size_class,
        }
    }
}

#[component]
pub(super) fn AgentBanner(chat: Chat) -> Element {
    let name = chat.header_name();
    let user_name = chat.user.name();
    let user_color = chat.user.color();
    rsx! {
        div { class: "flex items-center -space-x-2",
            Avatar {
                src: None,
                seed: user_name.clone(),
                background: user_color,
                alt: user_name.clone(),
                class: "h-10 w-10 border-2 border-zinc-100 text-xs dark:border-zinc-900".to_string(),
            }
            AgentAvatar { chat, size_class: "h-10 w-10 border-2 border-zinc-100 text-xs dark:border-zinc-900" }
        }
        h2 { class: "text-xl font-semibold capitalize tracking-tight text-foreground",
            "{name}"
        }
    }
}
