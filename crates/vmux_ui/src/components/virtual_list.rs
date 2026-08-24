use dioxus::prelude::*;
pub use dioxus_primitives::virtual_list::VirtualListProps;

#[component]
pub fn VirtualList(props: VirtualListProps) -> Element {
    rsx! {
        dioxus_primitives::virtual_list::VirtualList {
            count: props.count,
            buffer: props.buffer,
            estimate_size: props.estimate_size,
            render_item: props.render_item,
            attributes: props.attributes,
        }
    }
}
