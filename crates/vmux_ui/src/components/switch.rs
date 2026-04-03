use dioxus::prelude::*;
use dioxus_primitives::switch::{self, SwitchProps, SwitchThumbProps};

const SWITCH: &str = "group relative h-[1.15rem] w-8 cursor-pointer rounded-full border-0 bg-border transition-colors data-[state=checked]:bg-primary data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50";

const SWITCH_THUMB: &str = "block size-[calc(1.15rem-2px)] translate-x-px rounded-full bg-background transition-transform will-change-transform group-data-[state=checked]:translate-x-[calc(2rem-1px-(1.15rem-2px))] dark:bg-primary group-data-[state=checked]:dark:bg-card";

#[component]
pub fn Switch(props: SwitchProps) -> Element {
    rsx! {
        switch::Switch {
            class: SWITCH,
            checked: props.checked,
            default_checked: props.default_checked,
            disabled: props.disabled,
            required: props.required,
            name: props.name,
            value: props.value,
            on_checked_change: props.on_checked_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SwitchThumb(props: SwitchThumbProps) -> Element {
    rsx! {
        switch::SwitchThumb {
            class: SWITCH_THUMB,
            attributes: props.attributes,
            {props.children}
        }
    }
}
