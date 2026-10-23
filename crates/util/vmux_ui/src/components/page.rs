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
pub fn PageHeader(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] content_class: String,
) -> Element {
    let class = if class.is_empty() {
        "shrink-0 bg-foreground/[0.018] px-5 py-3".to_string()
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
