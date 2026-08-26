use dioxus::prelude::*;
use vmux_wire::room::ModelOptionEntry;

use crate::components::prompt_box::{PromptPopup, PromptPopupPlacement};
use crate::i18n::translate;

#[component]
pub fn ModelPill(name: String, on_open: EventHandler<()>) -> Element {
    if name.is_empty() {
        return rsx! {};
    }
    rsx! {
        button {
            class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
            title: translate("agent-change-model"),
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
