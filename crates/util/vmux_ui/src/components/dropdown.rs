use dioxus::prelude::*;

use crate::cn::cn;

#[component]
pub fn Dropdown(
    open: bool,
    #[props(default)] class: String,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let root_class = cn(["relative", class.as_str()]);
    rsx! {
        div {
            class: root_class,
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    on_dismiss.call(());
                }
            },
            if open {
                div {
                    class: "fixed inset-0 z-20",
                    aria_hidden: "true",
                    onmousedown: move |event: MouseEvent| {
                        event.prevent_default();
                        event.stop_propagation();
                        on_dismiss.call(());
                    },
                }
            }
            {children}
        }
    }
}
