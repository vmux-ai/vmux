use dioxus::prelude::*;
use vmux_wire::protocol::AcpModeOption;

use crate::components::prompt_box::{PromptMenuRow, PromptPopup, PromptPopupPlacement};
use crate::i18n::translate;
use crate::util::cn;

#[component]
pub fn PermissionMenu(
    #[props(default)] placement: PromptPopupPlacement,
    modes: Vec<AcpModeOption>,
    current_mode_id: String,
    selected: usize,
    on_hover: EventHandler<usize>,
    on_select: EventHandler<AcpModeOption>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        PromptPopup { placement, heading: translate("composer-permissions"), on_dismiss,
            for (index, mode) in modes.into_iter().enumerate() {
                button {
                    key: "permission-{mode.id}",
                    id: "agent-selector-item-{index}",
                    class: cn([
                        PromptMenuRow::class(index == selected).as_str(),
                        "flex-col items-stretch gap-0.5 text-left",
                    ]),
                    onmousedown: move |event| event.prevent_default(),
                    onmouseenter: move |_| on_hover.call(index),
                    onclick: {
                        let mode = mode.clone();
                        move |_| on_select.call(mode.clone())
                    },
                    div { class: "flex min-w-0 items-baseline gap-2",
                        span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{mode.name}" }
                        if mode.id == current_mode_id {
                            span { class: "shrink-0 text-[10px] uppercase tracking-wide text-primary", {translate("common-current")} }
                        }
                    }
                    if let Some(description) = &mode.description {
                        if !description.is_empty() {
                            span { class: "truncate text-xs text-muted-foreground", "{description}" }
                        }
                    }
                }
            }
        }
    }
}
