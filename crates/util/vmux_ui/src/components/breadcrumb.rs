use crate::i18n::translate;
use dioxus::prelude::*;

#[component]
pub fn Breadcrumbs(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] label: String,
) -> Element {
    let class = if class.is_empty() {
        "flex min-w-0 items-center gap-1".to_string()
    } else {
        class
    };
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
