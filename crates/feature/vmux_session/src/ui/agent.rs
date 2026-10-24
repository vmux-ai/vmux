use super::state::Chat;
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
use crate::{Route, SessionId, SessionSummary};

#[component]
pub(super) fn ChatHeader(
    chat: Chat,
    session: Option<SessionSummary>,
    repository: Option<SessionRepository>,
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
    let project_url = repository
        .as_ref()
        .map(|repository| repository.project_url.clone())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| "vmux://projects/".to_string());
    let project_seed = repository
        .as_ref()
        .map(|repository| repository.project_url.clone())
        .filter(|seed| !seed.is_empty())
        .or_else(|| session.as_ref().map(|session| session.cwd.clone()))
        .unwrap_or_else(|| project_name.clone());
    let session_url = session
        .as_ref()
        .map(|session| Route::Session(SessionId(session.id.clone())).url())
        .unwrap_or_else(|| "vmux://sessions/".to_string());
    let mut routes = vec![
        BreadcrumbOption {
            label: translate("resume-column-project"),
            value: project_url.clone(),
        },
        BreadcrumbOption {
            label: translate("sessions-title"),
            value: session_url,
        },
    ];
    if let Some(repository) = &repository
        && !repository.url.is_empty()
    {
        routes.push(BreadcrumbOption {
            label: translate("git-title"),
            value: repository.url.clone(),
        });
    }
    if let Some(session) = &session
        && let Ok(url) = url::Url::from_file_path(std::path::Path::new(&session.cwd))
    {
        routes.push(BreadcrumbOption {
            label: translate("schema-editor"),
            value: url.into(),
        });
    }
    rsx! {
        PageHeader {
            class: "session-chat-header relative z-10 shrink-0 bg-transparent px-5 pb-2 pt-[calc(0.75rem+env(safe-area-inset-top))]".to_string(),
            content_class: "mx-auto flex min-h-10 w-full max-w-3xl items-center gap-2".to_string(),
            Identicon {
                seed: project_seed,
                alt: project_name.clone(),
                class: "size-7 shrink-0 rounded-lg".to_string(),
            }
            Breadcrumbs { class: "min-w-0 flex-1".to_string(),
                BreadcrumbItem {
                    label: project_name,
                    title: project_url.clone(),
                    onclick: move |_| PageNavigation::replace(project_url.clone()),
                }
                BreadcrumbSeparator {}
                BreadcrumbSelector {
                    label: translate("sessions-title"),
                    options: routes,
                    onselect: move |url| PageNavigation::replace(url),
                }
                BreadcrumbSeparator {}
                BreadcrumbItem {
                    label: title.clone(),
                    title: title.clone(),
                    active: true,
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
