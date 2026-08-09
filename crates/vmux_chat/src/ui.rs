//! The chat page itself: one conversation's transcript, approvals and composer.
//!
//! Gated once here rather than per module, so what ships to wasm and iOS is this file and the
//! directory beside it. The desktop half that feeds it lives outside this crate, and speaks to
//! it only through the bin-ipc payloads in [`crate::event`].

#![allow(non_snake_case)]

mod keys;
mod scroll;
mod state;
mod tab;

use self::state::{Chat, use_chat};
use crate::clipboard::copy_to_clipboard;
use crate::event::{
    ChatAttachment, ChatCancelQueuedPrompt, ChatClearQueue, ChatCreateWorktree, ChatOpenPage,
    ChatPasteMedia, ChatPickFiles, ChatResume, ChatSelectWorkspace, SetAgentEffort,
    SlashCommandEntry,
};
use crate::format::composer::{ResumeMenuState, approval_decision_for_index, is_handoff_boundary};
use crate::transcript::{ChatItemRow, MD_CSS};
use dioxus::prelude::*;
#[cfg(web)]
use vmux_terminal::matrix_rain::MatrixRain;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_composer::{PROMPT_INPUT_ID, PromptComposer, focus_prompt_end};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

/// One agent conversation: its transcript, whatever it is waiting on, and the composer.
#[component]
pub fn Page(
    #[props(default)] agent_override: Option<String>,
    #[props(default)] transition_prompt: Option<String>,
    #[props(default)] transition_attachments: Option<Vec<ChatAttachment>>,
) -> Element {
    let chat = use_chat(agent_override, transition_prompt, transition_attachments);
    let accent = chat.accent();

    rsx! {
        main {
            class: "agent-chat-page relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent.css};",
            // Focusable so a click on the transcript lands focus here rather than on the body,
            // which would put keystrokes out of reach of the handler below. Deliberately not
            // autofocused: `focus_prompt_end` already claims focus for the prompt on mount.
            tabindex: "-1",
            onkeydown: move |event| chat.root_keydown(event),
            style { dangerous_inner_html: MD_CSS }
            if chat.installing_splash() {
                InstallBackdrop { accent_rgb: accent.rgb, title: chat.header_name().to_uppercase() }
            }
            ChatHeader { chat }
            ChatTranscript { chat }
            ChatApprovalPanel { chat }
            ChatDock { chat }
        }
    }
}

/// The falling-glyphs backdrop shown while an agent installs.
///
/// `MatrixRain` is a canvas animation and exists only on the CEF host. Installing an agent is a
/// desktop act anyway, so a native host renders nothing rather than an approximation.
#[cfg(web)]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    rsx! {
        div { class: "pointer-events-none absolute inset-0 z-0 overflow-hidden bg-background opacity-75",
            MatrixRain { accent_rgb, words: vec![title] }
        }
    }
}

#[cfg(not(web))]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    // The prop names have to match the CEF impl, since callers name them.
    let _ = (accent_rgb, title);
    rsx! {}
}

/// Who the conversation is with, and what it is called.
#[component]
fn ChatHeader(chat: Chat) -> Element {
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
fn StatusDot(status: String, size_class: String) -> Element {
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

/// The conversation itself, and the scroller that pages older messages in as it is read back.
#[component]
fn ChatTranscript(chat: Chat) -> Element {
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
                // Re-pin once the user reaches the bottom; unpin only when they scroll UP
                // (scroll_top decreases). Never unpin from our own programmatic
                // scroll-to-bottom, which only moves down and would otherwise poison
                // `at_bottom` with a stale, mid-stream scroll height.
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

/// The agent, front and centre, while it is being installed.
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

/// The same, once it is installed and waiting for a first prompt.
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
fn AgentBanner(chat: Chat) -> Element {
    let name = chat.header_name();
    rsx! {
        AgentAvatar { chat, size_class: "h-14 w-14 text-xl" }
        h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
            "{name}"
        }
    }
}

/// Where a handed-over conversation stops and this agent's own turns begin.
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

/// Why the agent stopped, and the way out when the cause is a bad package version.
#[component]
fn ChatErrorCard(message: String) -> Element {
    let is_startup = message.to_lowercase().contains("startup");
    let title = if is_startup {
        translate("agent-error-startup-title")
    } else {
        translate("common-error")
    };
    let copy_label = translate("common-copy");
    let copy_text = message.clone();
    rsx! {
        div { class: "flex flex-col gap-2 rounded-xl bg-red-500/[0.07] px-4 py-3 ring-1 ring-inset ring-red-500/20",
            div { class: "flex items-center gap-2",
                svg {
                    class: "h-4 w-4 shrink-0 text-red-500",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" }
                    path { d: "M12 9v4" }
                    path { d: "M12 17h.01" }
                }
                span { class: "text-sm font-semibold text-red-600 dark:text-red-300", "{title}" }
                button {
                    class: "ml-auto flex h-6 w-6 items-center justify-center rounded-md text-red-500/70 transition hover:bg-red-500/10 hover:text-red-500",
                    title: "{copy_label}",
                    aria_label: "{copy_label}",
                    onclick: move |_| copy_to_clipboard(&copy_text),
                    svg {
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                        path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                    }
                }
            }
            div { class: "max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-red-500/[0.06] px-3 py-2 font-mono text-[11px] leading-relaxed text-red-700/90 dark:text-red-200/80",
                "{message}"
            }
        }
        if is_version_error(&message) {
            div { class: "flex items-start gap-3 rounded-xl bg-foreground/[0.04] px-4 py-3 ring-1 ring-inset ring-foreground/10",
                div { class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/15 text-amber-500",
                    svg {
                        class: "h-4 w-4",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M9 18h6" }
                        path { d: "M10 22h4" }
                        path { d: "M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.3 1 2.1h6c0-.8.4-1.6 1-2.1A7 7 0 0 0 12 2Z" }
                    }
                }
                div { class: "flex min-w-0 flex-1 flex-col gap-2.5",
                    p { class: "text-sm leading-relaxed text-foreground", {translate("agent-error-version-suggestion")} }
                    button {
                        class: "vmux-gradient-outline inline-flex items-center gap-2 self-end rounded-xl px-6 py-3 text-sm font-semibold transition hover:-translate-y-0.5 hover:shadow-lg active:scale-[0.98]",
                        onclick: move |_| {
                            let _ = send(&ChatOpenPage { url: "vmux://agents".to_string() });
                        },
                        svg {
                            class: "h-4 w-4 text-indigo-500",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 3h6v6" }
                            path { d: "M10 14 21 3" }
                            path { d: "M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" }
                        }
                        span { class: "bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent",
                            {translate("agent-error-open-agents")}
                        }
                    }
                }
            }
        }
    }
}

/// Whether a startup/run error looks like a package registry/version block (npm 403, security
/// policy, forbidden version) — where the fix is usually pinning a different version.
fn is_version_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "403",
        "404",
        "forbidden",
        "security policy",
        "blocked",
        "eacces",
        "invalid tag",
        "einvalidtagname",
        "etarget",
        "no matching version",
        "notarget",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// The tool the agent is asking permission to run, and the three answers to it.
#[component]
fn ChatApprovalPanel(chat: Chat) -> Element {
    if chat.installing() {
        return rsx! {};
    }
    let Some((call_id, name, args_json)) = (chat.run.approval)() else {
        return rsx! {};
    };
    let approval_sel = chat.run.approval_sel;
    let details = crate::format::approval::ApprovalDetail::rows(&args_json);
    rsx! {
        div { class: "border-t border-foreground/10 bg-foreground/[0.04] px-4 py-3",
            div { class: "mx-auto flex max-w-3xl flex-col gap-3",
                div { class: "min-w-0",
                    div { class: "text-sm text-foreground",
                        {translate_with(
                            "agent-allow-tool",
                            &[("tool", TranslationValue::String(&name))],
                        )}
                    }
                    if !details.is_empty() {
                        div { class: "mt-2 max-h-40 overflow-auto rounded-lg bg-foreground/[0.05] ring-1 ring-inset ring-foreground/10",
                            for (i , detail) in details.iter().enumerate() {
                                div {
                                    key: "approval-detail-{i}",
                                    class: "grid grid-cols-[7rem_minmax(0,1fr)] items-start gap-3 border-b border-foreground/10 px-3 py-2 last:border-b-0",
                                    span { class: "pt-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70", "{approval_detail_label(&detail.label)}" }
                                    pre { class: "overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-muted-foreground", "{detail.value}" }
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-col gap-1.5",
                    for (index , label) in [translate("agent-allow"), translate("agent-allow-always"), translate("agent-deny")].into_iter().enumerate() {
                        button {
                            key: "approval-option-{index}",
                            class: if approval_sel() == index { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                            onclick: {
                                let call_id = call_id.clone();
                                move |_| {
                                    if let Some(decision) = approval_decision_for_index(index) {
                                        chat.answer_approval(call_id.clone(), decision);
                                    }
                                }
                            },
                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                            span { class: "min-w-0 flex-1", "{label}" }
                        }
                    }
                    div { class: "mt-1 text-[11px] text-muted-foreground", {translate("agent-choice-help").replace("1–9", "1–3")} }
                }
            }
        }
    }
}

fn approval_detail_label(label: &str) -> String {
    match label {
        "Details" => translate("agent-details"),
        "Path" => translate("agent-path"),
        "Tool" => translate("agent-tool"),
        "Server" => translate("agent-server"),
        _ => label.to_string(),
    }
}

/// Everything docked below the transcript: the pickers the draft opens, anything the agent is
/// waiting on an answer to, the queue, and the prompt itself.
#[component]
fn ChatDock(chat: Chat) -> Element {
    rsx! {
        div { class: "relative z-10 bg-gradient-to-t from-background via-background/95 to-transparent px-4 pb-4 pt-8",
            div { class: "agent-chat-prompt-shell vmux-agent-prompt-dock-enter relative mx-auto flex max-w-3xl flex-col gap-2",
                if chat.media_menu_open() {
                    MediaMenu { chat }
                }
                if chat.command_menu_open() {
                    CommandMenu { chat }
                }
                if chat.resume_menu_open() {
                    ResumeMenu { chat }
                }
                if chat.model_menu_open() {
                    ModelMenu { chat }
                }
                ChoiceList { chat }
                QueuedPrompts { chat }
                ChatComposer { chat }
            }
        }
    }
}

/// Files and folders matching the `@`-mention being typed.
#[component]
fn MediaMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup {
            PromptMediaOptions {
                items: chat.media_options(),
                selected: menu_sel(),
                loading: (chat.media.loading)(),
                loading_label: translate("agent-loading-media"),
                empty_label: translate("agent-no-matching-media"),
                on_hover: move |index| menu_sel.set(index),
                on_select: move |index| {
                    if let Some(entry) = chat.media.entries.peek().get(index).cloned() {
                        chat.select_media_entry(&entry);
                    }
                },
            }
        }
    }
}

/// The slash commands matching what has been typed after the `/`.
#[component]
fn CommandMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup {
            for (i , command) in chat.filtered_commands().into_iter().enumerate() {
                div {
                    key: "sc{i}",
                    id: "agent-selector-item-{i}",
                    class: if i == menu_sel() { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm bg-foreground/10" } else { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm" },
                    onmouseenter: move |_| menu_sel.set(i),
                    onclick: {
                        let name = command.name.clone();
                        move |_| chat.run_slash_command(&name)
                    },
                    span { class: "font-medium text-foreground", "/{command.name}" }
                    span { class: "text-xs text-muted-foreground", "{slash_command_description(&command)}" }
                }
            }
        }
    }
}

/// vmux's own commands describe themselves; an agent's own description stands.
fn slash_command_description(command: &SlashCommandEntry) -> String {
    match command.name.as_str() {
        "upload" => translate("agent-slash-attach-files"),
        "resume" => translate("agent-slash-resume-session"),
        "model" => translate("agent-slash-select-model"),
        "cli" => translate("agent-slash-continue-cli"),
        _ => command.description.clone(),
    }
}

/// Earlier sessions this agent can pick back up.
#[component]
fn ResumeMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    let state = chat.resume_state();
    rsx! {
        PromptPopup {
            if state == Some(ResumeMenuState::Loading) {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-loading-sessions")} }
            } else if state == Some(ResumeMenuState::Empty) {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-resumable-sessions")} }
            } else if state == Some(ResumeMenuState::NoMatch) {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-matching-sessions")} }
            } else {
                for (i , session) in chat.filtered_sessions().into_iter().enumerate() {
                    div {
                        key: "rs{i}",
                        id: "agent-selector-item-{i}",
                        class: if i == menu_sel() { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                        onmouseenter: move |_| menu_sel.set(i),
                        onclick: {
                            let session = session.clone();
                            move |_| chat.select_resume_session(&session)
                        },
                        div { class: "flex min-w-0 items-baseline gap-2",
                            span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{session.title}" }
                            if !session.agent_name.is_empty() {
                                span { class: "max-w-[40%] shrink-0 truncate text-xs text-muted-foreground", "{session.agent_name}" }
                            }
                        }
                        span { class: "truncate text-xs text-muted-foreground", "{session_age_label(session.age_seconds)} · {session.subtitle}" }
                    }
                }
            }
        }
    }
}

fn session_age_label(seconds: u64) -> String {
    match seconds {
        0..=59 => translate("agent-session-just-now"),
        60..=3599 => translate_with(
            "agent-session-minutes-ago",
            &[("count", TranslationValue::Number((seconds / 60) as i64))],
        ),
        3600..=86399 => translate_with(
            "agent-session-hours-ago",
            &[("count", TranslationValue::Number((seconds / 3600) as i64))],
        ),
        _ => translate_with(
            "agent-session-days-ago",
            &[("count", TranslationValue::Number((seconds / 86400) as i64))],
        ),
    }
}

/// The models this agent offers, narrowed by what follows `/model`.
#[component]
fn ModelMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    let current_model_id = (chat.models.current_model_id)();
    let models = chat.filtered_models();
    rsx! {
        PromptPopup {
            if models.is_empty() {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-matching-models")} }
            } else {
                for (i , model) in models.into_iter().enumerate() {
                    div {
                        key: "model{i}",
                        id: "agent-selector-item-{i}",
                        class: if i == menu_sel() { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                        onmouseenter: move |_| menu_sel.set(i),
                        onclick: {
                            let model = model.clone();
                            move |_| chat.select_model(&model)
                        },
                        div { class: "flex min-w-0 items-baseline gap-2",
                            span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{model.name}" }
                            if model.id == current_model_id {
                                span { class: "shrink-0 text-[10px] uppercase tracking-wide text-success", {translate("common-current")} }
                            }
                        }
                        if !model.description.is_empty() {
                            span { class: "truncate text-xs text-muted-foreground", "{model.description}" }
                        }
                    }
                }
            }
        }
    }
}

/// A question the agent asked, with its numbered answers.
#[component]
fn ChoiceList(chat: Chat) -> Element {
    let options = (chat.run.choice_options)();
    if options.is_empty() {
        return rsx! {};
    }
    let mut menu_sel = chat.slash.menu_sel;
    let question = (chat.run.choice_question)();
    rsx! {
        div { class: "rounded-2xl border border-foreground/10 bg-foreground/[0.045] p-3.5 shadow-sm",
            div { class: "mb-3 text-sm font-medium text-foreground", "{question}" }
            div { class: "flex flex-col gap-1.5",
                for (index , option) in options.into_iter().enumerate() {
                    button {
                        key: "choice-{index}",
                        id: "agent-choice-item-{index}",
                        onmouseenter: move |_| menu_sel.set(index),
                        class: if index == menu_sel() { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                        onclick: move |_| chat.answer_choice(index),
                        span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                        span { class: "min-w-0 flex-1", "{option}" }
                    }
                }
            }
            div { class: "mt-2.5 text-[11px] text-muted-foreground", {translate("agent-choice-help")} }
        }
    }
}

/// Prompts typed while the agent was busy, waiting their turn.
#[component]
fn QueuedPrompts(chat: Chat) -> Element {
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

/// The prompt box, wired to the draft and to everything a keystroke in it can mean.
#[component]
fn ChatComposer(chat: Chat) -> Element {
    let accent = agent_accent(&chat.agent());
    rsx! {
        PromptComposer {
            value: chat.draft(),
            preview: (chat.composer.transition_preview)(),
            attachments: chat.composer_attachments(),
            show_examples: chat.show_examples(),
            placeholder: if chat.choice_pending() { translate("agent-choose-option") } else { translate("command-composer-placeholder") },
            accent_bg: accent.accent_bg.to_string(),
            accent_color: chat.accent().css,
            accent_gradient: accent.grad.to_string(),
            footer: Some(rsx! {
                ComposerFooter { chat }
            }),
            action: chat.prompt_action(),
            action_title: chat.prompt_action_title(),
            action_enabled: chat.prompt_action_enabled(),
            on_input: move |value| chat.edit_draft(value),
            on_keydown: move |event| chat.prompt_keydown(event),
            on_paste: move |_| {
                let _ = send(&ChatPasteMedia);
            },
            on_attach: move |_| {
                let _ = send(&ChatPickFiles);
            },
            on_remove_attachment: move |index| chat.remove_attachment(index),
            on_action: move |_| {
                if chat.streaming() {
                    chat.stop_or_flush();
                } else {
                    chat.submit();
                }
            },
        }
    }
}

/// The strip under the prompt: what the turn will run as on the left, how it is going on the right.
#[component]
fn ComposerFooter(chat: Chat) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center justify-between gap-1",
            div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
                ModelPill { chat }
                EffortMenu { chat }
                AccessPill { chat }
                WorkspacePills { chat }
            }
            ComposerStatus { chat }
        }
    }
}

/// The model in use, which clicking swaps by opening `/model`.
#[component]
fn ModelPill(chat: Chat) -> Element {
    let name = (chat.models.current_model)();
    if name.is_empty() {
        return rsx! {};
    }
    let mut draft = chat.composer.draft;
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        button {
            class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
            title: "Change model",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| {
                draft.set("/model ".to_string());
                menu_sel.set(0);
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            svg {
                class: "h-3.5 w-3.5 shrink-0",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 3l1.7 4.6L18 9.3l-4.3 1.7L12 16l-1.7-5L6 9.3l4.3-1.7L12 3Z" }
                path { d: "M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15Z" }
            }
            span { class: "truncate", "{name}" }
            svg {
                class: "h-3 w-3 shrink-0 opacity-50",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "m8 10 4 4 4-4" }
            }
        }
    }
}

/// How hard the agent is asked to think, for the agents that expose the choice.
#[component]
fn EffortMenu(chat: Chat) -> Element {
    let levels = (chat.effort.levels)();
    if levels.is_empty() {
        return rsx! {};
    }
    let mut menu_open = chat.effort.menu_open;
    let agent_key = (chat.effort.agent_key)();
    let selected = (chat.effort.current)();
    rsx! {
        div { class: "relative shrink-0",
            button {
                id: "chat-effort-trigger",
                class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
                title: translate("agent-effort-tooltip"),
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| {
                    let next = !menu_open();
                    menu_open.set(next);
                    focus_prompt_end(PROMPT_INPUT_ID);
                },
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 20a8 8 0 1 1 8-8" }
                    path { d: "M12 12l3.5-2" }
                }
                span { class: "truncate capitalize",
                    {if selected.is_empty() { translate("agent-effort") } else { selected.clone() }}
                }
                svg {
                    class: "h-3 w-3 shrink-0 opacity-50",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path { d: "m8 10 4 4 4-4" }
                }
            }
            if menu_open() {
                div { class: "absolute bottom-full left-0 z-20 mb-2 min-w-[9rem] rounded-2xl border border-foreground/10 bg-background/95 p-1.5 shadow-xl backdrop-blur-xl",
                    div { class: "px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60", {translate("agent-effort")} }
                    EffortOption {
                        level: None,
                        agent_key: agent_key.clone(),
                        selected: selected.is_empty(),
                        chat,
                    }
                    for level in levels.into_iter() {
                        EffortOption {
                            key: "effort-{level}",
                            level: Some(level.clone()),
                            agent_key: agent_key.clone(),
                            selected: level == selected,
                            chat,
                        }
                    }
                }
            }
        }
    }
}

/// One effort level, or `None` for letting the agent decide. Picking one applies it at once and
/// remembers it for this agent.
#[component]
fn EffortOption(level: Option<String>, agent_key: String, selected: bool, chat: Chat) -> Element {
    let mut current = chat.effort.current;
    let mut menu_open = chat.effort.menu_open;
    // A level is a lowercase id from the agent, so it is title-cased for display; the default
    // label is already prose in whichever locale it was translated into.
    let (label, label_class) = match &level {
        Some(level) => (level.clone(), "min-w-0 flex-1 truncate capitalize"),
        None => (translate("agent-effort-default"), "min-w-0 flex-1 truncate"),
    };
    let level = level.unwrap_or_default();
    rsx! {
        button {
            class: if selected { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-1.5 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-1.5 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| {
                current.set(level.clone());
                menu_open.set(false);
                let _ = send(&SetAgentEffort { agent_key: agent_key.clone(), level: level.clone() });
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            span { class: "{label_class}", "{label}" }
            if selected {
                svg { class: "h-3.5 w-3.5 shrink-0 text-success", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}

/// How many tools this session may run without asking.
#[component]
fn AccessPill(chat: Chat) -> Element {
    let auto_allow_count = chat.slash.composer_context.read().auto_allow_count;
    let label = if auto_allow_count == 0 {
        "Ask".to_string()
    } else {
        format!("Ask · {auto_allow_count} allowed")
    };
    rsx! {
        span {
            class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
            title: "Tools ask before protected actions; Allow always is remembered per agent, repository or working directory, and tool",
            svg {
                class: "h-3.5 w-3.5",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 3 5 6v5c0 4.8 2.9 8.2 7 10 4.1-1.8 7-5.2 7-10V6l-7-3Z" }
                path { d: "m9 12 2 2 4-4" }
            }
            "{label}"
        }
    }
}

/// Which project the turn will run in, and what its repository looks like.
#[component]
fn WorkspacePills(chat: Chat) -> Element {
    let context = (chat.slash.composer_context)();
    let workspace_label = if context.workspace_selected && !context.workspace_name.is_empty() {
        context.workspace_name.clone()
    } else {
        "Select project".to_string()
    };
    let workspace_title = if context.cwd.is_empty() {
        "Choose project".to_string()
    } else {
        format!("Choose project · {}", context.cwd)
    };
    let branch_title = if context.branch.is_empty() {
        "Git repository".to_string()
    } else {
        format!("Branch {}", context.branch)
    };
    let worktree_title = if context.base_ref.is_empty() {
        "Linked worktree".to_string()
    } else {
        format!("Worktree from {}", context.base_ref)
    };
    rsx! {
        if context.can_manage_workspace {
            button {
                class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground transition hover:bg-foreground/[0.08] hover:text-foreground",
                title: "{workspace_title}",
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| {
                    let _ = send(&ChatSelectWorkspace);
                    focus_prompt_end(PROMPT_INPUT_ID);
                },
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                }
                span { class: "truncate", "{workspace_label}" }
            }
        } else if !context.cwd.is_empty() {
            span {
                class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                title: "{context.cwd}",
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                }
                span { class: "truncate", "{workspace_label}" }
            }
        }
        if context.is_git_repo {
            span {
                class: "flex h-7 max-w-40 shrink-0 items-center gap-1.5 rounded-lg px-2 font-mono text-[10px] text-muted-foreground",
                title: "{branch_title}",
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    circle { cx: "6", cy: "5", r: "2" }
                    circle { cx: "6", cy: "19", r: "2" }
                    circle { cx: "18", cy: "12", r: "2" }
                    path { d: "M8 5h3a3 3 0 0 1 3 3v1a3 3 0 0 0 3 3" }
                    path { d: "M6 7v10" }
                }
                span { class: "truncate", if context.branch.is_empty() { "Git" } else { "{context.branch}" } }
            }
            if context.is_worktree {
                span {
                    class: "flex h-7 shrink-0 items-center gap-1 rounded-lg bg-violet-500/[0.08] px-2 text-[10px] font-medium text-violet-600 ring-1 ring-inset ring-violet-500/15 dark:text-violet-300",
                    title: "{worktree_title}",
                    "Worktree"
                }
            } else if context.can_manage_workspace {
                button {
                    class: "flex h-7 shrink-0 items-center gap-1 rounded-lg px-2 text-[10px] font-medium text-muted-foreground transition hover:bg-violet-500/[0.08] hover:text-violet-600 dark:hover:text-violet-300",
                    title: "Create or select a worktree for this project",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        let _ = send(&ChatCreateWorktree);
                        focus_prompt_end(PROMPT_INPUT_ID);
                    },
                    "+ Worktree"
                }
            }
            if context.uncommitted > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-amber-500", title: "Uncommitted changes", "● {context.uncommitted}" }
            }
            if context.ahead > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-sky-500", title: "Commits ahead of upstream", "↑{context.ahead}" }
            }
        } else if context.workspace_selected {
            span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70", "No Git" }
        }
    }
}

/// What the agent is doing right now, and how much is still outstanding.
#[component]
fn ComposerStatus(chat: Chat) -> Element {
    let status = chat.status();
    let run_label = match status.as_str() {
        "streaming" => "Running",
        "awaiting" => "Approval",
        "installing" => "Starting",
        "errored" => "Error",
        _ => "Ready",
    };
    let (active_subagents, active_tasks) = (chat.activity_counts)();
    let queued_count = chat.queue.queued.read().len();
    rsx! {
        div { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground",
            span { class: "flex h-7 items-center gap-1.5 rounded-lg px-2",
                StatusDot { status, size_class: "h-1.5 w-1.5" }
                "{run_label}"
            }
            if active_subagents > 0 {
                span { class: "flex h-7 items-center gap-1 rounded-lg bg-violet-500/[0.07] px-2 text-violet-600 dark:text-violet-300", title: "Active subagents",
                    svg {
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "9", cy: "8", r: "3" }
                        path { d: "M3.5 19a5.5 5.5 0 0 1 11 0" }
                        circle { cx: "17", cy: "9", r: "2.5" }
                        path { d: "M15.5 14.5A4.5 4.5 0 0 1 21 19" }
                    }
                    "{active_subagents}"
                }
            }
            if active_tasks > 0 {
                span { class: "flex h-7 items-center gap-1 rounded-lg px-2", title: "Open plan tasks", "{active_tasks} tasks" }
            }
            if queued_count > 0 {
                span { class: "flex h-7 items-center gap-1 rounded-lg px-2", title: "Queued prompts", "{queued_count} queued" }
            }
        }
    }
}
