use dioxus::prelude::*;

use crate::components::prompt_box::{PromptMenuRow, PromptPopup, PromptPopupPlacement};
use crate::favicon::Favicon;
use crate::i18n::translate;
use crate::util::cn;

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
    #[props(default)] cursor: usize,
    #[props(default)] on_hover: Option<EventHandler<usize>>,
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
                for (index , option) in options.into_iter().enumerate() {
                    AgentMenuRow {
                        key: "{option.url}",
                        current: option.url == selected_url,
                        at_cursor: index == cursor,
                        option,
                        on_hover: move |()| {
                            if let Some(hover) = on_hover {
                                hover.call(index);
                            }
                        },
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
    current: bool,
    at_cursor: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<String>,
) -> Element {
    let url = option.url.clone();
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
            onclick: move |_| on_pick.call(url.clone()),
            Favicon {
                favicon_url: String::new(),
                url: option.url.clone(),
                class: "h-6 w-6 shrink-0 rounded-md object-contain",
                globe_class: "h-6 w-6 shrink-0 opacity-0",
            }
            span { class: "min-w-0 flex-1 truncate", "{option.title}" }
            if current {
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
