//! Everything the reader types into, and everything that describes the turn it will start.
//!
//! [`ChatDock`] is the whole input region, not just the text field: the menus a draft opens sit
//! above the prompt, the footer under it, and all of them read and write the same draft. The
//! controls split three ways below — [`options`] for what the turn runs as, [`workspace`] for
//! where it runs, [`menu`] for what the draft summons — which is the same three groups a smaller
//! screen has to decide what to keep.

use self::menu::{CommandMenu, MediaMenu, ResumeMenu};
use self::options::{AccessPill, EffortMenu, ModelMenu, ModelPill};
use self::workspace::WorkspacePills;
use super::agent::StatusDot;
use super::approval::ChoiceList;
use super::keys::ChatKeys;
use super::state::Chat;
use super::transcript::QueuedPrompts;
use crate::event::{ChatPasteMedia, ChatPickFiles};
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::prompt_composer::PromptComposer;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

/// Everything docked below the transcript: the pickers the draft opens, anything the agent is
/// waiting on an answer to, the queue, and the prompt itself.
#[component]
pub(super) fn ChatDock(chat: Chat) -> Element {
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

/// The prompt box, wired to the draft and to everything a keystroke in it can mean.
#[component]
fn ChatComposer(chat: Chat) -> Element {
    let accent = agent_accent(&chat.agent());
    let keys = use_context::<ChatKeys>();
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
            on_keydown: move |event| keys.on_prompt_keydown(event),
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

mod menu;
mod options;
mod workspace;
