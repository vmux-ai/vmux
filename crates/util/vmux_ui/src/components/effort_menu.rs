use dioxus::prelude::*;

use crate::components::prompt_box::{PromptMenuRow, PromptPopup, PromptPopupPlacement};
use crate::i18n::translate;
use crate::util::cn;

#[component]
pub fn EffortMenu(
    #[props(default)] placement: PromptPopupPlacement,
    levels: Vec<String>,
    selected: String,
    #[props(default)] cursor: usize,
    #[props(default)] on_hover: Option<EventHandler<usize>>,
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
                current: selected.is_empty(),
                at_cursor: cursor == 0,
                on_hover: move |()| {
                    if let Some(hover) = on_hover {
                        hover.call(0);
                    }
                },
                on_pick: move |level| on_select.call(level),
            }
            for (index , level) in levels.into_iter().enumerate() {
                EffortOption {
                    key: "effort-{level}",
                    level: Some(level.clone()),
                    current: level == selected,
                    at_cursor: cursor == index + 1,
                    on_hover: move |()| {
                        if let Some(hover) = on_hover {
                            hover.call(index + 1);
                        }
                    },
                    on_pick: move |level| on_select.call(level),
                }
            }
        }
    }
}

#[component]
fn EffortOption(
    level: Option<String>,
    current: bool,
    at_cursor: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<String>,
) -> Element {
    let (label, label_class) = match &level {
        Some(level) => (level.clone(), "min-w-0 flex-1 truncate capitalize"),
        None => (translate("agent-effort-default"), "min-w-0 flex-1 truncate"),
    };
    let level = level.unwrap_or_default();
    let text = match current {
        true => "text-foreground",
        false => "text-foreground/75 hover:text-foreground",
    };
    let row_class = PromptMenuRow::class(at_cursor);
    rsx! {
        button {
            class: cn([row_class.as_str(), text]),
            onmousedown: move |event| event.prevent_default(),
            onmouseenter: move |_| on_hover.call(()),
            onclick: move |_| on_pick.call(level.clone()),
            span { class: "{label_class}", "{label}" }
            if current {
                svg { class: "h-3.5 w-3.5 shrink-0 text-success", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}
