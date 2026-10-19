use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedControlItem {
    pub id: String,
    pub label: String,
}

#[component]
pub fn SegmentedControl(
    active: String,
    items: Vec<SegmentedControlItem>,
    onselect: EventHandler<String>,
    #[props(default)] class: String,
) -> Element {
    rsx! {
        div {
            class: "inline-flex h-8 shrink-0 items-center rounded-lg bg-foreground/[0.045] p-0.5 {class}",
            role: "tablist",
            for item in items {
                button {
                    r#type: "button",
                    role: "tab",
                    aria_selected: item.id == active,
                    class: if item.id == active {
                        "flex h-7 items-center rounded-md bg-foreground/[0.10] px-2.5 text-[11px] font-semibold text-foreground"
                    } else {
                        "flex h-7 items-center rounded-md px-2.5 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.055] hover:text-foreground"
                    },
                    onclick: move |_| onselect.call(item.id.clone()),
                    "{item.label}"
                }
            }
        }
    }
}
