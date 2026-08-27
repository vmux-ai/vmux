use dioxus::prelude::*;

use crate::components::prompt_box::{
    PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE, PROMPT_MENU_ROW_SELECTED, PromptPopup,
    PromptPopupPlacement,
};
use crate::i18n::translate;

#[component]
pub fn EffortMenu(
    #[props(default)] placement: PromptPopupPlacement,
    levels: Vec<String>,
    selected: String,
    on_select: EventHandler<String>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    if levels.is_empty() {
        return rsx! {};
    }
    rsx! {
        PromptPopup { placement, heading: translate("agent-effort"), on_dismiss,
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
