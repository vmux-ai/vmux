use super::conversation::{Keys, Section, Turn};
use super::item::UserAttachment;
use super::state::Chat;
use crate::event::{ChatCancelQueuedPrompt, ChatClearQueue, ChatResume};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};
use vmux_api::prompt_media::ChatAttachment;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

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
pub(super) fn ChatTranscript(
    chat: Chat,
    #[props(default)] reserve_details_toggle: bool,
) -> Element {
    use_hook(vmux_ui::scroll::StickySections::install);
    let section_keys = use_hook(|| Rc::new(RefCell::new(Keys::default())));
    let mut scroll_container = chat.transcript.scroll_container;
    let at_bottom = chat.transcript.at_bottom;
    let transcript = chat.transcript.current();
    let setup = chat.setup_pending();
    let transcript_loading = !setup
        && chat.status() != "errored"
        && (chat.transcript_loading() || !chat.transcript.ready());
    let loaded_start = transcript.loaded_start;
    let history_loading = transcript.loading;
    let has_older = transcript.has_older;
    let items = transcript.items;
    let initial_scroll = chat.transcript.initial_scroll();
    let placement_pending = !transcript_loading && !items.is_empty() && initial_scroll.is_empty();
    let latest_index = loaded_start as usize + items.len().saturating_sub(1);
    let mut turns = Turn::group(items, loaded_start as usize);
    let setup_prompt = chat.setup_prompt();
    let user_name = chat.user.name();
    let user_color = chat.user.color();
    section_keys.borrow_mut().assign(&mut turns);
    rsx! {
        super::surface::Transcript {
            initial_scroll,
            placement_pending,
            at_bottom: at_bottom(),
            filled: transcript_loading,
            busy: chat.in_use(),
            reserve_details_toggle,
            on_mounted: move |e: MountedEvent| {
                scroll_container.set(Some(e.data()));
                if *at_bottom.peek() {
                    chat.transcript.follow();
                }
            },
            on_wheel: move |_| chat.transcript.scroll_input(),
            on_touchmove: move |_| chat.transcript.scroll_input(),
            on_scroll: move |event: ScrollEvent| chat.transcript.scrolled(&event),
            on_scrollend: move |event: ScrollEvent| chat.transcript.scrolled(&event),
            on_resize: move |_| {
                if *chat.transcript.initial_follow.peek() || *at_bottom.peek() { chat.transcript.follow(); }
            },
                if setup { super::setup::Welcome {} }
                if transcript_loading {
                    TranscriptSkeleton {}
                } else if loaded_start > 0 || has_older {
                    HistorySentinel { loading: history_loading }
                }
                if !transcript_loading {
                    if let Some(prompt) = setup_prompt {
                        super::item::UserBubble { avatar_name: user_name.clone(), avatar_color: user_color.clone(), copy_text: prompt.text.clone(),
                            "{prompt.text}"
                            QueuedAttachments { attachments: prompt.attachments }
                        }
                    }
                }
                if !transcript_loading && chat.status() == "authentication" {
                    super::authentication::Authentication { chat }
                } else if !transcript_loading && chat.status() == "errored" && setup {
                    super::setup::Turn { p { class: "text-sm leading-relaxed", {translate("onboarding-setup-failed")} } }
                } else if !transcript_loading && chat.setup_busy() {
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
pub(super) fn TranscriptSkeleton() -> Element {
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
    let phase = if chat.installing() || chat.installing_splash() || chat.status() == "installing" {
        super::setup::Phase::Installation
    } else {
        super::setup::Phase::Preparation
    };
    rsx! {
        super::setup::ProgressTurn { phase, detail, percent: chat.install_percent() }
    }
}

#[component]
fn ReadyIntro(chat: Chat) -> Element {
    rsx! {
        super::setup::Turn {
            p { class: "text-sm leading-relaxed",
                {translate_with("onboarding-ready", &[("agent", TranslationValue::String(&chat.header_name()))])}
            }
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
    let mut queued = chat.queue.queued();
    if chat.setup_prompt().is_some() {
        queued.remove(0);
    }
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
                vmux_ui::components::composer::ShortcutHint { shortcut: "Esc", label: translate("agent-send-all-now") }
            }
        }
    }
}
