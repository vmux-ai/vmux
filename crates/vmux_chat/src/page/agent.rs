//! Who the conversation is with: the header strip, and the marks the agent is recognised by.
//!
//! The avatar, the status dot and the banner are worn at three sizes in three places — the
//! header, the transcript's intro and the composer's status strip — so the agent's likeness is
//! described once here rather than redrawn at each of them.

use super::state::Chat;
use dioxus::prelude::*;
use vmux_ui::favicon::favicon_src_for_url;

/// Who the conversation is with, and what it is called.
#[component]
pub(super) fn ChatHeader(chat: Chat) -> Element {
    let name = chat.header_name();
    let title = chat.title();
    rsx! {
        header { class: "agent-chat-header vmux-agent-surface-enter relative z-10 flex min-w-0 items-center gap-2.5 border-b bg-background/95 px-3 py-3 shadow-[0_1px_0_rgba(255,255,255,0.02)] sm:px-5",
            AgentAvatar { chat, size_class: "h-6 w-6 text-[11px]" }
            StatusDot { status: chat.status(), size_class: "h-2.5 w-2.5" }
            div { class: "min-w-0 flex-1",
                div {
                    class: "truncate bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-sm font-semibold text-transparent",
                    title: "{title}",
                    "{title}"
                }
                div { class: "truncate text-[10px] text-muted-foreground/60", "{name}" }
            }
        }
    }
}

/// The agent's face: its favicon when it has one, else an initial on its accent.
#[component]
fn AgentAvatar(chat: Chat, size_class: String) -> Element {
    let agent = chat.agent();
    let accent = (chat.identity.accent)();
    let src = favicon_src_for_url(
        &(chat.identity.agent_icon)(),
        &format!("vmux://agent/{agent}"),
    );
    let initial: String = chat
        .header_name()
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_default();
    let fallback = if accent.is_empty() {
        "#6366f1"
    } else {
        &accent
    };
    let style = if src.is_some() {
        String::new()
    } else {
        format!("background:{fallback}")
    };
    rsx! {
        div {
            class: "flex shrink-0 items-center justify-center overflow-hidden rounded-full font-semibold text-white {size_class}",
            style: "{style}",
            if let Some(src) = src.as_ref() {
                img { class: "h-full w-full object-cover", src: "{src}" }
            } else {
                "{initial}"
            }
        }
    }
}

/// The coloured dot that says at a glance what the agent is doing.
#[component]
pub(super) fn StatusDot(status: String, size_class: String) -> Element {
    let tone = match status.as_str() {
        "streaming" => "bg-amber-400 shadow-[0_0_8px_rgba(251,191,36,0.65)]",
        "installing" => "bg-sky-400 shadow-[0_0_8px_rgba(56,189,248,0.65)]",
        "awaiting" => "bg-violet-400 shadow-[0_0_8px_rgba(167,139,250,0.65)]",
        "errored" => "bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.65)]",
        _ => "bg-success shadow-[0_0_8px_rgba(16,185,129,0.65)]",
    };
    rsx! {
        span { class: "{size_class} rounded-full {tone}" }
    }
}

/// The same face at full size, over the agent's name, for a transcript with nothing in it yet.
#[component]
pub(super) fn AgentBanner(chat: Chat) -> Element {
    let name = chat.header_name();
    rsx! {
        AgentAvatar { chat, size_class: "h-14 w-14 text-xl" }
        h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
            "{name}"
        }
    }
}
