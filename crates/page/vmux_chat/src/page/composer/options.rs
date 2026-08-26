use crate::event::{ModelOptionEntry, SetAgentEffort};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn ChatModelPill(chat: Chat) -> Element {
    let mut draft = chat.composer.draft;
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        ModelPill {
            name: (chat.models.current_model)(),
            on_open: move |_| {
                draft.set("/model ".to_string());
                menu_sel.set(0);
                focus_prompt_end(PROMPT_INPUT_ID);
            },
        }
    }
}

#[component]
pub fn ModelPill(name: String, on_open: EventHandler<()>) -> Element {
    if name.is_empty() {
        return rsx! {};
    }
    rsx! {
        button {
            class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
            title: "Change model",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_open.call(()),
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

#[component]
pub(super) fn ChatModelMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        ModelMenu {
            models: chat.filtered_models(),
            current_model_id: (chat.models.current_model_id)(),
            selected: menu_sel(),
            on_hover: move |index| menu_sel.set(index),
            on_select: move |model: ModelOptionEntry| chat.select_model(&model),
            on_dismiss: move |()| chat.dismiss_selector(),
        }
    }
}

#[component]
pub fn ModelMenu(
    models: Vec<ModelOptionEntry>,
    current_model_id: String,
    selected: usize,
    on_hover: EventHandler<usize>,
    on_select: EventHandler<ModelOptionEntry>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        PromptPopup { on_dismiss,
            if models.is_empty() {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-matching-models")} }
            } else {
                for (i , model) in models.into_iter().enumerate() {
                    div {
                        key: "model{i}",
                        id: "agent-selector-item-{i}",
                        class: if i == selected { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                        onmouseenter: move |_| on_hover.call(i),
                        onclick: {
                            let model = model.clone();
                            move |_| on_select.call(model.clone())
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

#[component]
pub(super) fn ChatEffortMenu(chat: Chat) -> Element {
    let mut current = chat.effort.current;
    let agent_key = (chat.effort.agent_key)();
    rsx! {
        EffortMenu {
            levels: (chat.effort.levels)(),
            selected: current(),
            on_select: move |level: String| {
                current.set(level.clone());
                let _ = send(&SetAgentEffort { agent_key: agent_key.clone(), level });
                focus_prompt_end(PROMPT_INPUT_ID);
            },
        }
    }
}

#[component]
pub fn EffortMenu(
    levels: Vec<String>,
    selected: String,
    on_select: EventHandler<String>,
) -> Element {
    if levels.is_empty() {
        return rsx! {};
    }
    let mut menu_open = use_signal(|| false);
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
                        selected: selected.is_empty(),
                        on_pick: move |level| {
                            menu_open.set(false);
                            on_select.call(level);
                        },
                    }
                    for level in levels.into_iter() {
                        EffortOption {
                            key: "effort-{level}",
                            level: Some(level.clone()),
                            selected: level == selected,
                            on_pick: move |level| {
                                menu_open.set(false);
                                on_select.call(level);
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EffortOption(level: Option<String>, selected: bool, on_pick: EventHandler<String>) -> Element {
    let (label, label_class) = match &level {
        Some(level) => (level.clone(), "min-w-0 flex-1 truncate capitalize"),
        None => (translate("agent-effort-default"), "min-w-0 flex-1 truncate"),
    };
    let level = level.unwrap_or_default();
    rsx! {
        button {
            class: if selected { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-1.5 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-1.5 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_pick.call(level.clone()),
            span { class: "{label_class}", "{label}" }
            if selected {
                svg { class: "h-3.5 w-3.5 shrink-0 text-success", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}
