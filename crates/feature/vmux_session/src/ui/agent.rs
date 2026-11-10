use super::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn DetailsToggle(
    details_available: bool,
    details_open: bool,
    on_details_toggle: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "absolute right-3 top-2 z-30 flex size-7 items-center justify-center rounded-md bg-background/90 text-muted-foreground transition-colors hover:bg-card hover:text-foreground disabled:opacity-35",
            title: translate("agent-details"),
            aria_label: translate("agent-details"),
            aria_expanded: details_open,
            disabled: !details_available,
            onclick: move |_| on_details_toggle.call(()),
            LineIconView { icon: LineIcon::Layout, class: "size-4" }
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
