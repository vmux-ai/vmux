use dioxus::prelude::*;

use crate::PromptBoxTone;

/// One selectable Mac file or directory in the prompt media menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptMediaOption {
    pub key: String,
    pub name: String,
    pub display_path: String,
    pub preview_data_url: String,
    pub label: String,
    pub is_dir: bool,
}

#[component]
/// Shared media selector shown for an open `@` token.
pub fn PromptMediaOptions(
    items: Vec<PromptMediaOption>,
    selected: usize,
    loading: bool,
    #[props(default)] tone: PromptBoxTone,
    #[props(default = "Loading media…".to_string())] loading_label: String,
    #[props(default = "No matching media".to_string())] empty_label: String,
    on_select: EventHandler<usize>,
    on_hover: EventHandler<usize>,
) -> Element {
    let muted_class = match tone {
        PromptBoxTone::Adaptive => "text-muted-foreground",
        PromptBoxTone::Dark => "text-zinc-500",
    };
    if loading {
        return rsx! {
            div { class: "px-3.5 py-2 text-sm {muted_class}", "{loading_label}" }
        };
    }
    if items.is_empty() {
        return rsx! {
            div { class: "px-3.5 py-2 text-sm {muted_class}", "{empty_label}" }
        };
    }

    let selected_class = match tone {
        PromptBoxTone::Adaptive => {
            "flex cursor-pointer items-center gap-3 bg-foreground/10 px-3.5 py-2"
        }
        PromptBoxTone::Dark => "flex cursor-pointer items-center gap-3 bg-white/10 px-3.5 py-2",
    };
    let item_class = "flex cursor-pointer items-center gap-3 px-3.5 py-2";
    let preview_class = match tone {
        PromptBoxTone::Adaptive => {
            "flex h-12 w-16 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-foreground/[0.06] text-muted-foreground ring-1 ring-inset ring-foreground/10"
        }
        PromptBoxTone::Dark => {
            "flex h-12 w-16 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-white/[0.06] text-zinc-500 ring-1 ring-inset ring-white/10"
        }
    };
    let name_class = match tone {
        PromptBoxTone::Adaptive => "truncate text-sm text-foreground",
        PromptBoxTone::Dark => "truncate text-sm text-zinc-100",
    };

    rsx! {
        for (index, item) in items.iter().cloned().enumerate() {
            div {
                key: "{item.key}",
                id: "prompt-media-item-{index}",
                class: if index == selected { selected_class } else { item_class },
                onmousedown: move |event| event.prevent_default(),
                onmouseenter: move |_| on_hover.call(index),
                onclick: move |_| on_select.call(index),
                div { class: preview_class,
                    if item.is_dir {
                        svg {
                            class: "h-4 w-4",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M3 6h6l2 2h10v10H3z" }
                        }
                    } else if !item.preview_data_url.is_empty() {
                        img {
                            src: "{item.preview_data_url}",
                            alt: "{item.name}",
                            class: "h-full w-full object-contain",
                        }
                    } else {
                        span { class: "font-mono text-[9px] font-semibold", "{item.label}" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: name_class, "{item.name}" }
                    div { class: "truncate text-xs {muted_class}", "{item.display_path}" }
                }
            }
        }
    }
}
