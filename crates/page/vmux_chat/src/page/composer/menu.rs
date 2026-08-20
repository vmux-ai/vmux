//! What typing in the prompt summons over it: files behind `@`, commands behind `/`, and the
//! earlier sessions behind `/resume`.
//!
//! All three are the same gesture — keep typing to narrow, arrow to pick — and all three read the
//! one selection index the keyboard handler drives, so they move together.

use crate::event::SlashCommandEntry;
use crate::format::composer::ResumeMenuState;
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

/// Files and folders matching the `@`-mention being typed.
#[component]
pub(super) fn MediaMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
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
pub(super) fn CommandMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
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
pub(super) fn ResumeMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    let state = chat.resume_state();
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
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
