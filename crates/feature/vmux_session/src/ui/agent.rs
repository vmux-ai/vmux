use super::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar, IdentityOrb};
use vmux_ui::components::breadcrumb::{BreadcrumbOption, BreadcrumbSelector, BreadcrumbSeparator};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::components::manager::{WorkspaceBreadcrumbHeader, WorkspacePage};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::navigation::PageNavigation;

use crate::event::SessionRepository;
use crate::state::Navigation;
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
    pub(super) fn from_navigation(navigation: &Navigation) -> Self {
        let projects = navigation
            .projects
            .iter()
            .map(|project| BreadcrumbOption {
                label: project.name.clone(),
                value: project.url.clone(),
                identity_seed: project.url.clone(),
                icon_url: String::new(),
                icon: None,
            })
            .collect();
        let sessions = navigation
            .sessions
            .iter()
            .map(|session| ChatHeaderSession {
                project_url: session.project_url.clone(),
                option: BreadcrumbOption {
                    label: session.title.clone(),
                    value: Route::Session(SessionId(session.id.clone())).url(),
                    identity_seed: session.id.clone(),
                    icon_url: String::new(),
                    icon: None,
                },
            })
            .collect();
        Self { projects, sessions }
    }

    pub(super) fn navigation(
        &self,
        repository: Option<&SessionRepository>,
        session: Option<&SessionSummary>,
    ) -> ChatHeaderNavigation {
        let current_project = repository.map(|repository| repository.project_url.as_str());
        let sessions = self
            .sessions
            .iter()
            .filter(|session| current_project.is_none_or(|project| session.project_url == project))
            .map(|session| session.option.clone())
            .collect();
        let mut contextual = Vec::new();
        if let Some(session) = session {
            if let Some(repository) = repository {
                if !repository.url.is_empty() {
                    contextual.push(BreadcrumbOption {
                        label: translate("git-title"),
                        value: repository.url.clone(),
                        identity_seed: String::new(),
                        icon_url: "assets/favicons/git.svg".to_string(),
                        icon: Some(vmux_api::BuiltinIcon::GitBranch),
                    });
                }
            }
            if !session.cwd.is_empty()
                && let Ok(url) = url::Url::from_directory_path(&session.cwd)
            {
                contextual.push(BreadcrumbOption {
                    label: translate("editor-editor"),
                    value: url.to_string(),
                    identity_seed: String::new(),
                    icon_url: "assets/favicons/editor.svg".to_string(),
                    icon: Some(vmux_api::BuiltinIcon::Code),
                });
            }
        }
        let views = WorkspacePage::options_with(contextual);
        ChatHeaderNavigation {
            projects: self.projects.clone(),
            sessions,
            views,
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct ChatHeaderNavigation {
    projects: Vec<BreadcrumbOption>,
    sessions: Vec<BreadcrumbOption>,
    views: Vec<BreadcrumbOption>,
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
        WorkspaceBreadcrumbHeader {
            current: WorkspacePage::Sessions.option(),
            options: navigation.views,
            actions: rsx! {
                div { class: "flex items-center gap-2",
                    StatusDot { status: chat.status(), size_class: "h-2 w-2" }
                    Avatar {
                        src: None,
                        seed: user_name.clone(),
                        background: user_color,
                        alt: user_name.clone(),
                        class: "size-5 shrink-0 text-[7px]".to_string(),
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
            },
            BreadcrumbSeparator {}
            BreadcrumbSelector {
                label: project_name.clone(),
                class: "max-w-[min(16rem,30vw)] shrink-0".to_string(),
                options: navigation.projects,
                leading: rsx! {
                    IdentityOrb {
                        seed: project_seed,
                        alt: project_name.clone(),
                        class: "size-5".to_string(),
                    }
                },
                onselect: move |url| PageNavigation::replace(url),
            }
            BreadcrumbSeparator {}
            BreadcrumbSelector {
                label: title.clone(),
                class: "min-w-0 flex-1".to_string(),
                active: true,
                options: navigation.sessions,
                leading: rsx! {
                    IdentityOrb {
                        seed: session_seed,
                        alt: title.clone(),
                        class: "size-5".to_string(),
                    }
                },
                onselect: move |url| PageNavigation::replace(url),
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
