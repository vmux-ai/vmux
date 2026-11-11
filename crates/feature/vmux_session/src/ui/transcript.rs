use super::agent::AgentBanner;
use super::conversation::{Keys, Section, Turn};
use super::item::UserAttachment;
use super::state::Chat;
use crate::event::{ChatCancelQueuedPrompt, ChatClearQueue, ChatResume};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};
use vmux_api::prompt_media::ChatAttachment;
use vmux_ui::agent_accent::AgentAccent;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::scroll::ScrollPreload;

#[component]
fn QueuedAttachments(attachments: Vec<ChatAttachment>) -> Element {
    let mut thumbs = Vec::new();
    let mut plain = Vec::new();
    let mut videos = Vec::new();
    for attachment in attachments {
        if attachment.mime_type.starts_with("video/") && !attachment.media_url.is_empty() {
            videos.push(attachment);
            continue;
        }
        match attachment.preview_data_url.is_empty() {
            true => plain.push(attachment.name),
            false => thumbs.push((
                attachment.path,
                attachment.name,
                attachment.preview_data_url,
            )),
        }
    }
    rsx! {
        if !videos.is_empty() {
            div { class: "mt-1 flex flex-wrap gap-2",
                for attachment in videos {
                    UserAttachment { attachment, onload: move |_| {} }
                }
            }
        }
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
    use_hook(vmux_ui::scroll::StickySections::install);
    let section_keys = use_hook(|| Rc::new(RefCell::new(Keys::default())));
    let mut scroll_container = chat.transcript.scroll_container;
    let at_bottom = chat.transcript.at_bottom;
    let transcript = chat.transcript.current();
    let transcript_loading =
        chat.status() != "errored" && (chat.transcript_loading() || !chat.transcript.ready());
    let loaded_start = transcript.loaded_start;
    let history_loading = transcript.loading;
    let items = transcript.items;
    let initial_scroll = chat.transcript.initial_scroll();
    let placement_pending = !transcript_loading && !items.is_empty() && initial_scroll.is_empty();
    let latest_index = loaded_start as usize + items.len().saturating_sub(1);
    let mut turns = Turn::group(items, loaded_start as usize);
    section_keys.borrow_mut().assign(&mut turns);
    rsx! {
        div {
            id: "chat-scroll",
            "data-vmux-sticky-container": "",
            "data-vmux-initial-scroll": initial_scroll,
            "data-vmux-presentation-pending": placement_pending.then_some(""),
            "data-vmux-follow-scroll": at_bottom().to_string(),
            "data-vmux-preserve-scroll": "auto",
            onmounted: move |e| {
                scroll_container.set(Some(e.data()));
                if *at_bottom.peek() {
                    chat.transcript.follow();
                }
            },
            class: "relative z-10 flex-1 overflow-y-auto overscroll-contain scroll-pt-16 px-3 pb-20 pt-3 outline-none sm:px-4 md:px-6",
            tabindex: "0",
            onwheel: move |_| chat.transcript.note_scroll_input(),
            ontouchmove: move |_| chat.transcript.note_scroll_input(),
            onscroll: move |e: Event<ScrollData>| {
                if e.client_height() <= 0 {
                    return;
                }
                let top = e.scroll_top() as i32;
                let dist = e.scroll_height() - top - e.client_height();
                chat.transcript.observe_scroll(top, dist, e.client_height());
                if ScrollPreload::HISTORY.reached_start(&e) {
                    chat.transcript.request_history();
                }
            },
            div {
                class: "mx-auto flex min-h-full w-full max-w-3xl flex-col gap-5",
                style: transcript_loading.then_some("height:100%;"),
                onresize: move |_| {
                    if *chat.transcript.initial_follow.peek() || *at_bottom.peek() {
                        chat.transcript.follow();
                    }
                },
                if transcript_loading {
                    TranscriptSkeleton {}
                } else if loaded_start > 0 {
                    HistorySentinel { loading: history_loading }
                }
                if !transcript_loading && chat.installing_splash() {
                    InstallIntro { chat, detail: chat.install_detail() }
                } else if !transcript_loading && turns.is_empty() && chat.status() == "idle" {
                    ReadyIntro { chat }
                }
                if !transcript_loading {
                    for turn in turns {
                        Section { key: "{turn.key()}", turn, latest_index, chat }
                    }
                }
                div {
                    id: "chat-bottom",
                    class: "h-px shrink-0",
                    onmounted: move |_| {
                        if *at_bottom.peek() {
                            chat.transcript.follow();
                        }
                    },
                }
                if chat.queue.paused() {
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
fn HistorySentinel(loading: bool) -> Element {
    rsx! {
        div { class: "flex h-8 shrink-0 items-center justify-center",
            if loading {
                div { class: "flex w-28 flex-col gap-1.5",
                    Skeleton { class: "h-1.5 w-full bg-foreground/[0.07]" }
                    Skeleton { class: "h-1.5 w-3/4 bg-foreground/[0.05]" }
                }
            }
        }
    }
}

#[component]
fn TranscriptSkeleton() -> Element {
    rsx! {
        div { class: "flex min-h-full shrink-0 flex-col justify-end gap-5 px-1 py-1", "data-vmux-loading-surface": "",
            div { class: "ml-auto flex w-2/3 items-start justify-end gap-3",
                div { class: "flex min-w-0 flex-1 flex-col items-end gap-2 pt-1",
                    Skeleton { class: "h-2.5 w-12 bg-foreground/[0.07]" }
                    Skeleton { class: "h-3 w-40 bg-foreground/[0.08]" }
                }
                Skeleton { class: "size-6 shrink-0 rounded-full bg-foreground/[0.08]" }
            }
            div { class: "flex w-3/4 items-start gap-3",
                Skeleton { class: "size-6 shrink-0 rounded-full bg-foreground/[0.08]" }
                div { class: "flex min-w-0 flex-1 flex-col gap-2 pt-1",
                    Skeleton { class: "h-2.5 w-14 bg-foreground/[0.07]" }
                    Skeleton { class: "h-3 w-full bg-foreground/[0.08]" }
                    Skeleton { class: "h-3 w-4/5 bg-foreground/[0.07]" }
                    Skeleton { class: "h-3 w-3/5 bg-foreground/[0.06]" }
                }
            }
            div { class: "ml-auto flex w-1/2 items-start justify-end gap-3",
                div { class: "flex min-w-0 flex-1 flex-col items-end gap-2 pt-1",
                    Skeleton { class: "h-2.5 w-12 bg-foreground/[0.07]" }
                    Skeleton { class: "h-3 w-32 bg-foreground/[0.08]" }
                }
                Skeleton { class: "size-6 shrink-0 rounded-full bg-foreground/[0.08]" }
            }
        }
    }
}

#[component]
fn InstallIntro(chat: Chat, detail: String) -> Element {
    let accent = AgentAccent::for_agent(&chat.agent());
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
pub(super) fn HandoffDivider(source: String, truncated: bool) -> Element {
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
    let queued = chat.queue.queued();
    if queued.is_empty() {
        return rsx! {};
    }
    let paused = chat.queue.paused();
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
                        if !queued_prompt.attachments.is_empty() {
                            QueuedAttachments {
                                attachments: queued_prompt.attachments.clone(),
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
