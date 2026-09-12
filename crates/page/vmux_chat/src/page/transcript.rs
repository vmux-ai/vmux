use super::agent::AgentBanner;
use super::error::ChatErrorCard;
use super::state::Chat;
use crate::event::{ChatCancelQueuedPrompt, ChatClearQueue, ChatResume};
use crate::format::composer::is_handoff_boundary;
use crate::transcript::ChatItemRow;
use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_wire::prompt_media::ChatAttachment;

#[component]
fn QueuedAttachments(
    names: Vec<String>,
    paths: Vec<String>,
    previews: Signal<HashMap<String, ChatAttachment>>,
) -> Element {
    let held = previews.read();
    let mut thumbs = Vec::new();
    let mut plain = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let path = paths.get(index).cloned().unwrap_or_default();
        let preview = held
            .get(&path)
            .map(|preview| preview.preview_data_url.clone())
            .unwrap_or_default();
        match preview.is_empty() {
            true => plain.push(name.clone()),
            false => thumbs.push((path, name.clone(), preview)),
        }
    }
    rsx! {
        if !thumbs.is_empty() {
            div { class: "mt-1 flex flex-wrap gap-1.5",
                for (path , name , preview) in thumbs {
                    img {
                        key: "q-thumb-{path}",
                        src: "{preview}",
                        alt: "{name}",
                        title: "{name}",
                        loading: "lazy",
                        decoding: "async",
                        class: "h-16 w-auto max-w-48 rounded-lg object-cover",
                    }
                }
            }
        }
        if !plain.is_empty() {
            span { class: "block text-xs text-foreground/45",
                {format!("{} {}", translate("agent-attached"), plain.join(", "))}
            }
        }
    }
}

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
    let agent = chat.agent();
    let agent_name = chat.header_name();
    let agent_avatar = favicon_src_for_url(
        &(chat.identity.agent_icon)(),
        &format!("vmux://sessions/{agent}"),
    );
    let agent_color = chat.accent().css;
    let user_name = (chat.user.name)();
    let user_color = (chat.user.color)();
    rsx! {
        div {
            id: "chat-scroll",
            onmounted: move |e| scroll_container.set(Some(e.data())),
            class: "relative z-10 flex-1 overflow-y-auto overscroll-contain px-3 pb-8 pt-3 sm:px-4 md:px-6",
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
            div { class: "mx-auto flex min-h-full w-full max-w-3xl flex-col gap-5",
                if loaded_start() > 0 {
                    button {
                        id: "chat-load-older",
                        class: "mx-auto mb-3 px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50",
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
                        agent_name: agent_name.clone(),
                        agent_avatar: agent_avatar.clone(),
                        agent_color: agent_color.clone(),
                        user_name: user_name.clone(),
                        user_color: user_color.clone(),
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
        div { class: "my-auto flex flex-col items-center gap-3 py-12 text-center",
            AgentBanner { chat }
            div { class: "flex max-w-sm items-center gap-2 text-xs text-muted-foreground",
                span { class: "h-1.5 w-1.5 shrink-0 rounded-full {accent.accent_bg}" }
                span { class: "truncate", "{detail}" }
            }
        }
    }
}

#[component]
fn ReadyIntro(chat: Chat) -> Element {
    rsx! {
        div { class: "flex flex-col items-center gap-3 py-16 text-center",
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
                    class: "group flex max-w-[80%] items-center gap-2 border-l-2 border-dashed border-foreground/20 py-2 pl-3 pr-2 text-sm text-muted-foreground",
                    span { class: "shrink-0 text-[10px] uppercase tracking-wide text-foreground/40", {translate("agent-queued")} }
                    span { class: "min-w-0 flex-1 whitespace-pre-wrap break-words",
                        if !queued_prompt.text.is_empty() {
                            "{queued_prompt.text}"
                        }
                        if !queued_prompt.attachment_names.is_empty() {
                            QueuedAttachments {
                                names: queued_prompt.attachment_names.clone(),
                                paths: queued_prompt.attachment_paths.clone(),
                                previews: chat.composer.attachment_previews,
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
