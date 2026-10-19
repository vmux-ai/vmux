use dioxus::prelude::*;

#[component]
pub fn BreadcrumbSeparator() -> Element {
    rsx! {
        span { class: "shrink-0 text-xs text-muted-foreground/45", "/" }
    }
}
