use super::agent::AgentBanner;
use super::error::ChatErrorCard;
use super::state::Chat;
use crate::event::{ChatCancelQueuedPrompt, ChatClearQueue, ChatResume};
use crate::format::composer::is_handoff_boundary;
use crate::transcript::ChatItemRow;
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub(super) fn ChatTranscript(chat: Chat) -> Element {
    let mut scroll_container = chat.transcript.scroll_container;
    let mut at_bottom = chat.transcript.at_bottom;
    let mut last_top = chat.transcript.last_top;
    let loaded_start = chat.transcript.loaded_start;
    let history_loading = chat.transcript.history_loading;
    let items = chat.transcript.items;
    let latest_tool = (chat.latest_tool)();
    let handoff_source = (chat.handoff.source)();
    let handoff_truncated = (chat.handoff.truncated)();
    let handoff_count = (chat.handoff.message_count)();
    rsx! {
        div {
            id: "chat-scroll",
            onmounted: move |e| scroll_container.set(Some(e.data())),
            class: "vmux-agent-surface-enter vmux-agent-surface-enter-delayed relative z-10 flex-1 overflow-y-auto overscroll-contain px-3 py-6 sm:px-4 md:px-6",
            onscroll: move |e: Event<ScrollData>| {
                let top = e.scroll_top() as i32;
                let dist = e.scroll_height() - top - e.client_height();
                if dist <= 48 {
                    at_bottom.set(true);
                } else if top < *last_top.peek() - 4 {
                    at_bottom.set(false);
                }
                last_top.set(top);
                if top <= 160 {
                    chat.request_history();
                }
            },
            div { class: "mx-auto flex min-h-full max-w-none flex-col gap-5 md:max-w-3xl",
                if loaded_start() > 0 {
                    button {
                        id: "chat-load-older",
                        class: "mx-auto rounded-full border border-foreground/10 bg-background/90 px-3 py-1.5 text-xs text-muted-foreground shadow-sm transition-colors hover:bg-foreground/[0.06] hover:text-foreground disabled:opacity-50",
                        disabled: history_loading(),
                        onclick: move |_| chat.request_history(),
                        {if history_loading() { translate("agent-loading-older") } else { translate("agent-load-older") }}
                    }
                }
                if chat.installing_splash() {
                    InstallIntro { chat, detail: chat.install_detail() }
                } else if items.read().is_empty() && chat.status() == "idle" {
                    ReadyIntro { chat }
                }
                for (i , item) in items.read().iter().cloned().enumerate() {
                    ChatItemRow {
                        key: "{loaded_start() as usize + i}",
                        absolute_index: loaded_start() as usize + i,
                        item,
                        attachment_previews: chat.composer.attachment_previews,
                        latest_tool_block: latest_tool
                            .filter(|(item_index, _)| *item_index == i)
                            .map(|(_, block_index)| block_index),
                    }
                    if !handoff_source.is_empty()
                        && is_handoff_boundary(loaded_start() as usize + i, handoff_count)
                    {
                        HandoffDivider { source: handoff_source.clone(), truncated: handoff_truncated }
                    }
                }
                if chat.status() == "errored" {
                    ChatErrorCard { message: (chat.run.error)() }
                }
                if (chat.queue.paused)() {
                    div { class: "flex items-center gap-3 py-1 text-xs text-muted-foreground",
                        span { class: "h-px flex-1 bg-foreground/10" }
                        span { class: "shrink-0", {translate("agent-interrupted")} }
                        span { class: "h-px flex-1 bg-foreground/10" }
                    }
                }
            }
        }
    }
}

#[component]
fn InstallIntro(chat: Chat, detail: String) -> Element {
    let accent = agent_accent(&chat.agent());
    rsx! {
        div { class: "my-auto flex flex-col items-center gap-3 py-16 text-center",
            AgentBanner { chat }
            div { class: "flex max-w-sm items-center gap-2 rounded-full bg-background/90 px-3 py-1.5 text-xs text-muted-foreground ring-1 ring-inset ring-foreground/10",
                span { class: "h-1.5 w-1.5 shrink-0 rounded-full {accent.accent_bg}" }
                span { class: "truncate", "{detail}" }
            }
        }
    }
}

#[component]
fn ReadyIntro(chat: Chat) -> Element {
    rsx! {
        div { class: "vmux-agent-ready-enter flex flex-col items-center gap-3 py-24 text-center",
            AgentBanner { chat }
            p { class: "text-sm text-muted-foreground", {translate("agent-ready")} }
        }
    }
}

#[component]
fn HandoffDivider(source: String, truncated: bool) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 py-1 text-xs text-muted-foreground",
            span { class: "h-px flex-1 bg-foreground/10" }
            span {
                {translate_with(
                    "agent-continued-from",
                    &[("source", TranslationValue::String(&source))],
                )}
            }
            if truncated {
                span { class: "text-amber-500/80", {format!("· {}", translate("agent-older-context-omitted"))} }
            }
            span { class: "h-px flex-1 bg-foreground/10" }
        }
    }
}

#[component]
pub(super) fn QueuedPrompts(chat: Chat) -> Element {
    let queued = (chat.queue.queued)();
    if !chat.composer.transition_preview.read().is_empty() || queued.is_empty() {
        return rsx! {};
    }
    let paused = (chat.queue.paused)();
    let count = queued.len();
    rsx! {
        div { class: "flex flex-col items-end gap-1.5",
            for queued_prompt in queued.into_iter() {
                div {
                    key: "q{queued_prompt.id}",
                    class: "group flex max-w-[80%] items-center gap-2 rounded-2xl border border-dashed border-foreground/20 bg-foreground/[0.03] py-2 pl-3.5 pr-2 text-sm text-muted-foreground",
                    span { class: "shrink-0 text-[10px] uppercase tracking-wide text-foreground/40", {translate("agent-queued")} }
                    span { class: "min-w-0 flex-1 whitespace-pre-wrap break-words",
                        if !queued_prompt.text.is_empty() {
                            "{queued_prompt.text}"
                        }
                        if !queued_prompt.attachment_names.is_empty() {
                            span { class: "block text-xs text-foreground/45",
                                {format!("{} ", translate("agent-attached"))}
                                for (i , name) in queued_prompt.attachment_names.iter().enumerate() {
                                    if i > 0 { ", " }
                                    "{name}"
                                }
                            }
                        }
                    }
                    button {
                        class: "flex shrink-0 items-center rounded-lg p-1 text-foreground/35 opacity-70 transition hover:bg-foreground/10 hover:text-foreground hover:opacity-100 focus:opacity-100",
                        title: translate("agent-cancel-queued"),
                        onclick: move |_| {
                            let _ = send(&ChatCancelQueuedPrompt { id: queued_prompt.id });
                        },
                        svg {
                            class: "h-3.5 w-3.5",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            path { d: "M6 6l12 12M18 6L6 18" }
                        }
                    }
                }
            }
            if paused {
                div { class: "flex items-center gap-1",
                    button {
                        class: "flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                        title: translate("agent-resume-queued"),
                        onclick: move |_| {
                            let _ = send(&ChatResume);
                        },
                        svg {
                            class: "h-3.5 w-3.5",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path { d: "M8 5v14l11-7z" }
                        }
                        span { class: "tabular-nums", "{count}" }
                    }
                    button {
                        class: "flex items-center rounded-lg p-1 text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                        title: translate("agent-clear-queue"),
                        onclick: move |_| {
                            let _ = send(&ChatClearQueue);
                        },
                        svg {
                            class: "h-3.5 w-3.5",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            path { d: "M6 6l12 12M18 6L6 18" }
                        }
                    }
                }
            }
            div { class: "flex items-center gap-2 pr-1 text-[10px] text-foreground/40",
                kbd { class: "inline-flex h-5 items-center rounded border border-foreground/15 bg-foreground/[0.06] px-1.5 font-mono text-[10px] font-medium text-foreground/60 shadow-sm", "Esc" }
                span { {translate("agent-send-all-now")} }
            }
        }
    }
}
