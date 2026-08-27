use dioxus::prelude::*;
use vmux_wire::room::ModelOptionEntry;

use crate::components::prompt_box::{
    PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE, PROMPT_MENU_ROW_SELECTED, PromptPopup,
    PromptPopupPlacement,
};
use crate::i18n::translate;

#[component]
pub fn ModelMenu(
    #[props(default)] placement: PromptPopupPlacement,
    models: Vec<ModelOptionEntry>,
    current_model_id: String,
    selected: usize,
    on_hover: EventHandler<usize>,
    on_select: EventHandler<ModelOptionEntry>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        PromptPopup { placement, on_dismiss,
            if models.is_empty() {
                div { class: "{PROMPT_MENU_ROW} text-muted-foreground", {translate("agent-no-matching-models")} }
            } else {
                for (i , model) in models.into_iter().enumerate() {
                    div {
                        key: "model{i}",
                        id: "agent-selector-item-{i}",
                        class: if i == selected { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_SELECTED} cursor-pointer flex-col items-stretch gap-0.5") } else { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE} cursor-pointer flex-col items-stretch gap-0.5") },
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
