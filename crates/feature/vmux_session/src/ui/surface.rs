use dioxus::prelude::*;
pub(super) use vmux_ui::components::conversation::{Dock, Transcript};

#[component]
pub(super) fn Page(
    accent: String,
    #[props(default)] on_keydown: Option<EventHandler<KeyboardEvent>>,
    children: Element,
) -> Element {
    rsx! {
        vmux_ui::components::conversation::Page {
            accent, on_keydown, stylesheet: super::item::MD_CSS.to_string(),
            {children}
        }
    }
}
