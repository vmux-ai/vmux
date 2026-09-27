use dioxus::prelude::*;
use dioxus_primitives::switch::{self, SwitchProps, SwitchThumbProps};

const SWITCH: &str = "group relative h-6 w-10 cursor-pointer rounded-full border-0 bg-muted shadow-[inset_0_0_0_1px_var(--border)] transition-colors hover:bg-muted/70 data-[state=checked]:bg-primary data-[state=checked]:shadow-none data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background";

const SWITCH_THUMB: &str = "absolute left-0.5 top-0.5 block size-5 rounded-full bg-foreground/80 shadow-sm transition-transform will-change-transform group-data-[state=checked]:translate-x-4 group-data-[state=checked]:bg-background";

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
