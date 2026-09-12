use super::state::Chat;
use dioxus::prelude::*;
use vmux_ui::back::BackButton;
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::i18n::{TranslationValue, translate_with};

#[component]
pub(super) fn ChatHeader(chat: Chat) -> Element {
    let name = chat.header_name();
    let title = chat.title();
    let subtitle = translate_with(
        "agent-chat-subtitle",
        &[("agent", TranslationValue::String(&name))],
    );
    let user_name = (chat.user.name)();
    let user_color = (chat.user.color)();
    rsx! {
        header { class: "session-chat-header relative z-10 flex min-w-0 items-center gap-3 bg-transparent px-3 pb-2 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-5",
            BackButton {}
            div { class: "relative h-8 w-12 shrink-0",
                Avatar {
                    src: None,
                    seed: user_name.clone(),
                    background: user_color,
                    alt: user_name.clone(),
                    class: "absolute left-0 top-0 h-8 w-8 border-2 border-zinc-100 text-[10px] dark:border-zinc-900".to_string(),
                }
                AgentAvatar { chat, size_class: "absolute left-5 top-0 h-8 w-8 border-2 border-zinc-100 text-[10px] dark:border-zinc-900" }
            }
            div { class: "min-w-0 flex-1",
                div {
                    class: "truncate text-sm font-semibold text-foreground",
                    title: "{title}",
                    "{title}"
                }
                div { class: "flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground",
                    StatusDot { status: chat.status(), size_class: "h-2 w-2" }
                    span { class: "truncate", "{subtitle}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn AgentAvatar(chat: Chat, size_class: String) -> Element {
    let agent = chat.agent();
    let accent = (chat.identity.accent)();
    let src = favicon_src_for_url(
        &(chat.identity.agent_icon)(),
        &format!("vmux://sessions/{agent}"),
    );
    let fallback = if accent.is_empty() {
        "#6366f1"
    } else {
        &accent
    };
    rsx! {
        Avatar {
            src,
            seed: chat.header_name(),
            background: fallback,
            alt: chat.header_name(),
            class: size_class,
        }
    }
}

#[component]
pub(super) fn AgentBanner(chat: Chat) -> Element {
    let name = chat.header_name();
    let user_name = (chat.user.name)();
    let user_color = (chat.user.color)();
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
