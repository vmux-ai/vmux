use dioxus::prelude::*;

use crate::icon::{LineIcon, LineIconView};

#[component]
pub fn FilterTrigger(active: bool, label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active {
                "flex size-5 shrink-0 items-center justify-center rounded text-[11px] font-medium text-foreground bg-foreground/[0.09]"
            } else {
                "flex size-5 shrink-0 items-center justify-center rounded text-[11px] font-medium text-muted-foreground/65 hover:bg-foreground/[0.06] hover:text-foreground"
            },
            title: label.clone(),
            aria_label: label,
            aria_pressed: active,
            onclick: move |event| onclick.call(event),
            "#"
        }
    }
}

#[component]
pub fn FilterPill(
    label: String,
    #[props(default)] dot: String,
    clear_label: String,
    onclear: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        span { class: "flex h-5 max-w-56 shrink-0 items-center gap-1.5 rounded-md bg-foreground/[0.07] pl-1.5 pr-0.5 text-[10px] text-foreground ring-1 ring-inset ring-foreground/[0.09]",
            span { class: "text-muted-foreground", "#" }
            if !dot.is_empty() {
                span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            }
            span { class: "truncate", "{label}" }
            button {
                r#type: "button",
                class: "flex size-4 shrink-0 items-center justify-center rounded text-xs text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground",
                title: clear_label.clone(),
                aria_label: clear_label,
                onclick: move |event| onclear.call(event),
                "×"
            }
        }
    }
}

#[component]
pub fn FilterMenu(
    title: String,
    #[props(default)] onback: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "absolute left-0 right-0 top-8 z-30 overflow-hidden rounded-md bg-background shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
            if !title.is_empty() {
                header { class: "flex h-8 items-center gap-1 border-b border-foreground/[0.07] px-1.5",
                    if let Some(onback) = onback {
                        button {
                            r#type: "button",
                            class: "flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                            onclick: move |event| onback.call(event),
                            LineIconView {
                                icon: LineIcon::ChevronDown,
                                class: "size-3 rotate-90".to_string(),
                            }
                        }
                    }
                    span { class: "min-w-0 flex-1 truncate px-1 text-[10px] font-medium text-muted-foreground", "{title}" }
                }
            }
            div { class: "max-h-72 overflow-y-auto p-1", {children} }
        }
    }
}

#[component]
pub fn FilterOption(
    label: String,
    #[props(default)] detail: String,
    #[props(default)] prefix: String,
    #[props(default)] dot: String,
    #[props(default)] trailing: String,
    #[props(default)] next: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            "data-vmux-dropdown-option": "",
            class: "flex min-h-8 w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground outline-none hover:bg-foreground/[0.05] hover:text-foreground focus:bg-foreground/[0.05] focus:text-foreground data-[vmux-dropdown-active=true]:bg-foreground/[0.08] data-[vmux-dropdown-active=true]:text-foreground",
            onclick: move |event| onclick.call(event),
            if !prefix.is_empty() {
                span { class: "w-3.5 shrink-0 text-center text-muted-foreground/60", "{prefix}" }
            }
            if !dot.is_empty() {
                span { class: "flex w-3.5 shrink-0 items-center justify-center",
                    span { class: "size-2 rounded-full {dot}" }
                }
            }
            span { class: "min-w-0 flex-1",
                span { class: "block truncate", "{label}" }
                if !detail.is_empty() {
                    span { class: "block truncate text-[9px] text-muted-foreground/55", "{detail}" }
                }
            }
            if !trailing.is_empty() {
                span { class: "shrink-0 text-[10px] tabular-nums text-muted-foreground/60", "{trailing}" }
            }
            if next {
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: "size-3 shrink-0 -rotate-90 text-muted-foreground/45".to_string(),
                }
            }
        }
    }
}
