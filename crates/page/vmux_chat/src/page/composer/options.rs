use crate::event::{ModelOptionEntry, SetAgentEffort};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::model_menu::{ModelMenu, ModelPill};
use vmux_ui::components::prompt_box::{
    PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE, PROMPT_MENU_ROW_SELECTED, PromptPopup,
};
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
pub(super) fn ChatEffortPill(chat: Chat) -> Element {
    rsx! {
        EffortPill {
            levels: (chat.effort.levels)(),
            selected: (chat.effort.current)(),
            on_open: move |()| {
                chat.open_only(crate::page::state::ComposerMenu::Effort);
                chat.effort.toggle();
            },
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
                chat.effort.close();
                current.set(level.clone());
                let _ = send(&SetAgentEffort { agent_key: agent_key.clone(), level });
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            on_dismiss: move |()| chat.effort.close(),
        }
    }
}

#[component]
pub fn EffortPill(levels: Vec<String>, selected: String, on_open: EventHandler<()>) -> Element {
    if levels.is_empty() {
        return rsx! {};
    }
    rsx! {
        button {
            id: "chat-effort-trigger",
            class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
            title: translate("agent-effort-tooltip"),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| {
                on_open.call(());
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
    }
}

#[component]
pub fn EffortMenu(
    levels: Vec<String>,
    selected: String,
    on_select: EventHandler<String>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    if levels.is_empty() {
        return rsx! {};
    }
    rsx! {
        PromptPopup { on_dismiss,
            div { class: "px-3 pb-1 pt-1.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60", {translate("agent-effort")} }
            EffortOption {
                level: None,
                selected: selected.is_empty(),
                on_pick: move |level| on_select.call(level),
            }
            for level in levels.into_iter() {
                EffortOption {
                    key: "effort-{level}",
                    level: Some(level.clone()),
                    selected: level == selected,
                    on_pick: move |level| on_select.call(level),
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
            class: if selected { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_SELECTED} text-foreground") } else { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE} text-foreground/75 hover:text-foreground") },
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
