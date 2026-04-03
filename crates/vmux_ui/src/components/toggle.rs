use dioxus::prelude::*;
use dioxus_primitives::toggle::{self, ToggleProps};

const TOGGLE: &str = "inline-flex min-w-8 w-fit items-center justify-center rounded-lg border-0 bg-transparent px-2 py-0 text-sm text-muted-foreground outline-none hover:cursor-pointer hover:bg-accent focus-visible:cursor-pointer focus-visible:bg-accent data-[state=on]:bg-primary data-[state=on]:text-foreground";

#[component]
pub fn Toggle(props: ToggleProps) -> Element {
    rsx! {
        toggle::Toggle {
            class: TOGGLE,
            pressed: props.pressed,
            default_pressed: props.default_pressed,
            disabled: props.disabled,
            on_pressed_change: props.on_pressed_change,
            onmounted: props.onmounted,
            onfocus: props.onfocus,
            onkeydown: props.onkeydown,
            attributes: props.attributes,
            {props.children}
        }
    }
}
