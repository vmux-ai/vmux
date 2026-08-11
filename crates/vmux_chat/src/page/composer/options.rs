//! What the turn will run as: which model, how hard it is asked to think, and what it may do
//! without asking.
//!
//! A pill and the menu it opens are one control, so they are described together — the model pill
//! opens `/model` over the prompt and the effort pill opens its own popover, but either way the
//! reader is choosing one setting.

use crate::event::SetAgentEffort;
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

/// The model in use, which clicking swaps by opening `/model`.
#[component]
pub(super) fn ModelPill(chat: Chat) -> Element {
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

/// The models this agent offers, narrowed by what follows `/model`.
#[component]
pub(super) fn ModelMenu(chat: Chat) -> Element {
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

/// How hard the agent is asked to think, for the agents that expose the choice.
#[component]
pub(super) fn EffortMenu(chat: Chat) -> Element {
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
pub(super) fn AccessPill(chat: Chat) -> Element {
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
