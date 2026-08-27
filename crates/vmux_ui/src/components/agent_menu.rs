use dioxus::prelude::*;

use crate::components::prompt_box::{
    PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE, PROMPT_MENU_ROW_SELECTED, PromptPopup,
    PromptPopupPlacement,
};
use crate::i18n::translate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerAgentOption {
    pub url: String,
    pub title: String,
}

#[component]
pub fn AgentMenu(
    #[props(default)] placement: PromptPopupPlacement,
    options: Vec<ComposerAgentOption>,
    selected_url: String,
    on_select: EventHandler<String>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        PromptPopup {
            placement,
            heading: translate("composer-agent"),
            on_dismiss,
            id: "start-agent-selector",
            div { class: "p-1.5",
                for option in options.into_iter() {
                    AgentMenuRow {
                        key: "{option.url}",
                        selected: option.url == selected_url,
                        option,
                        on_pick: move |url| on_select.call(url),
                    }
                }
            }
        }
    }
}

#[component]
fn AgentMenuRow(
    option: ComposerAgentOption,
    selected: bool,
    on_pick: EventHandler<String>,
) -> Element {
    let url = option.url.clone();
    let initial = option.title.chars().next().unwrap_or('A');
    rsx! {
        button {
            class: if selected { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_SELECTED} text-foreground") } else { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE} text-foreground/75 hover:text-foreground") },
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_pick.call(url.clone()),
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.07] text-[10px] font-semibold uppercase", "{initial}" }
            span { class: "min-w-0 flex-1 truncate", "{option.title}" }
            if selected {
                svg {
                    class: "h-3.5 w-3.5 shrink-0 text-success",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2.2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}
