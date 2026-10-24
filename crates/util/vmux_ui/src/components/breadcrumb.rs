use crate::cn::cn;
use crate::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;

#[derive(Clone, PartialEq, Eq)]
pub struct BreadcrumbOption {
    pub label: String,
    pub value: String,
}

#[component]
pub fn Breadcrumbs(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] label: String,
) -> Element {
    let class = cn(["flex min-w-0 items-center gap-1", class.as_str()]);
    let label = if label.is_empty() {
        translate("editor-breadcrumb")
    } else {
        label
    };
    rsx! {
        nav { class, aria_label: label, {children} }
    }
}

#[component]
pub fn BreadcrumbItem(
    label: String,
    #[props(default)] title: String,
    #[props(default)] active: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    let class = if active {
        "flex h-7 min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs font-medium text-foreground"
    } else {
        "flex h-7 min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
    };
    match onclick {
        Some(handler) => rsx! {
            button {
                r#type: "button",
                class,
                title,
                onclick: move |event| handler.call(event),
                {leading}
                span { class: "truncate", "{label}" }
            }
        },
        None => rsx! {
            span { class, title, aria_current: if active { "page" } else { "false" },
                {leading}
                span { class: "truncate", "{label}" }
            }
        },
    }
}

#[component]
pub fn BreadcrumbSeparator() -> Element {
    rsx! {
        span { class: "shrink-0 text-xs text-muted-foreground/45", "/" }
    }
}

#[component]
pub fn BreadcrumbSelector(
    label: String,
    options: Vec<BreadcrumbOption>,
    onselect: EventHandler<String>,
    #[props(default)] class: String,
    #[props(default)] active: bool,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let root_class = cn(["min-w-0", class.as_str()]);
    let button_class = if active {
        "flex h-7 w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs font-medium text-foreground transition-colors hover:bg-foreground/[0.05]"
    } else {
        "flex h-7 w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
    };
    rsx! {
        Dropdown {
            open: open(),
            keyboard_navigation: true,
            class: root_class,
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: button_class,
                aria_expanded: open(),
                onclick: move |_| open.set(!open()),
                {leading}
                span { class: "truncate", "{label}" }
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: if open() {
                        "size-3 shrink-0 rotate-180 transition-transform".to_string()
                    } else {
                        "size-3 shrink-0 transition-transform".to_string()
                    },
                }
            }
            if open() {
                DropdownPopup { align: DropdownAlign::Start, class: "w-40 p-1",
                    for option in options {
                        button {
                            key: "{option.value}",
                            r#type: "button",
                            "data-vmux-dropdown-option": "",
                            class: "flex w-full items-center rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground data-[vmux-dropdown-active=true]:bg-foreground/[0.07] data-[vmux-dropdown-active=true]:text-foreground",
                            onclick: move |_| {
                                open.set(false);
                                onselect.call(option.value.clone());
                            },
                            span { class: "truncate", "{option.label}" }
                        }
                    }
                }
            }
        }
    }
}
