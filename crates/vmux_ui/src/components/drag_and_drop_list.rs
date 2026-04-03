use dioxus::prelude::*;
use dioxus_primitives::drag_and_drop_list::{
    self, DragAndDropContext, DragAndDropItemContext, DragAndDropListItemProps,
};
use dioxus_primitives::icon::Icon;

#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropListProps {
    /// Items (labels) to be rendered.
    pub items: Vec<Element>,

    /// Set if the list items should be removable
    #[props(default)]
    pub is_removable: bool,

    /// Accessible label for the list
    #[props(default)]
    pub aria_label: Option<String>,

    /// Additional attributes to apply to the list element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the list component.
    pub children: Element,
}

#[component]
pub fn DragAndDropList(props: DragAndDropListProps) -> Element {
    let is_removable = props.is_removable;
    let items = props
        .items
        .iter()
        .map(|item| {
            rsx! {
                DragIcon {}
                div { class: "mr-4 min-w-0 flex-1 text-base font-normal leading-6 text-muted-foreground", {item} }
                if is_removable {
                    RemoveButton {}
                }
            }
        })
        .collect();

    rsx! {
        Fragment {
            style { "{include_str!(\"drag_and_drop_list.css\")}" }
            drag_and_drop_list::DragAndDropList {
                items,
                aria_label: props.aria_label,
                attributes: props.attributes,
                {props.children}
            }
        }
    }
}

#[component]
pub fn DragAndDropListItem(props: DragAndDropListItemProps) -> Element {
    rsx! {
        drag_and_drop_list::DragAndDropListItem {
            index: props.index,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
fn DragIcon() -> Element {
    rsx! {
        div { class: "mr-4 flex w-6 shrink-0 items-center text-muted-foreground", aria_hidden: "true",
            Icon {
                // equal icon from lucide https://lucide.dev/icons/equal
                stroke: "var(--secondary-color-4)",
                line { x1: "5", x2: "19", y1: "9", y2: "9" }
                line { x1: "5", x2: "19", y1: "15", y2: "15" }
            }
        }
    }
}

#[component]
pub fn RemoveButton(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let mut ctx: DragAndDropContext = use_context();
    let item_ctx: DragAndDropItemContext = use_context();
    let index = item_ctx.index();
    let label = format!("Remove item {}", index + 1);
    rsx! {
        button {
            class: "ml-4 flex w-6 cursor-pointer items-center overflow-visible border-none bg-transparent p-0 focus-visible:rounded-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
            aria_label: "{label}",
            onclick: move |_| ctx.remove(index),
            ..attributes,
            {children}
            Icon {
                // X icon from lucide https://lucide.dev/icons/x
                stroke: "var(--secondary-color-4)",
                path { d: "M18 6 6 18" }
                path { d: "m6 6 12 12" }
            }
        }
    }
}
