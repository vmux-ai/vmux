use dioxus::prelude::*;

#[component]
pub fn PageShell(
    children: Element,
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    rsx! {
        main {
            class: "flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-sans text-sm outline-none",
            onkeydown: move |event| {
                if let Some(handler) = &onkeydown {
                    handler.call(event);
                }
            },
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn PageToolbarRow(
    children: Element,
    #[props(default = "gap-2 px-3".to_string())] class: String,
) -> Element {
    rsx! {
        div { class: "flex h-8 min-w-0 shrink-0 items-center font-sans text-xs text-muted-foreground {class}",
            {children}
        }
    }
}

#[component]
pub fn PageSidebarToggle(open: bool, label: String, ontoggle: EventHandler<()>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "group/sidebar-toggle flex size-6 shrink-0 cursor-default items-center justify-center rounded-md text-foreground transition-[background-color,transform] duration-100 hover:bg-foreground/[0.045] active:scale-95 motion-reduce:transition-none",
            title: label.clone(),
            aria_label: label,
            aria_expanded: open,
            onclick: move |_| ontoggle.call(()),
            svg {
                class: "h-4 w-4 opacity-60 transition-opacity duration-100 group-hover/sidebar-toggle:opacity-100 motion-reduce:transition-none",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M5 3h4v18H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z", fill: "currentColor", stroke: "none",
                    class: if open {
                        "origin-[3px_12px] scale-x-100 opacity-100 transition-[transform,opacity] duration-100 motion-reduce:transition-none"
                    } else {
                        "origin-[3px_12px] scale-x-0 opacity-0 transition-[transform,opacity] duration-100 motion-reduce:transition-none"
                    },
                }
                path { d: "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z M9 3v18" }
            }
        }
    }
}

#[component]
pub fn PageHeader(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] content_class: String,
) -> Element {
    let class = if class.is_empty() {
        "shrink-0 px-5 py-3".to_string()
    } else {
        class
    };
    let content_class = if content_class.is_empty() {
        "mx-auto w-full max-w-5xl".to_string()
    } else {
        content_class
    };
    rsx! {
        header { class: "{class}",
            div { class: "{content_class}", {children} }
        }
    }
}

#[component]
pub fn PageContent(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] content_class: String,
) -> Element {
    let class = if class.is_empty() {
        "min-h-0 flex-1 overflow-auto px-5 py-5".to_string()
    } else {
        class
    };
    let content_class = if content_class.is_empty() {
        "mx-auto w-full max-w-5xl".to_string()
    } else {
        content_class
    };
    rsx! {
        div { class: "{class}",
            div { class: "{content_class}", {children} }
        }
    }
}
