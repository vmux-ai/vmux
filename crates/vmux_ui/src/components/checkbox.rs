use dioxus::prelude::*;
use dioxus_primitives::checkbox::{self, CheckboxProps};
use dioxus_primitives::icon;

const CHECKBOX: &str = "size-4 box-border cursor-pointer rounded border-0 bg-card p-0 text-muted-foreground shadow-[inset_0_0_0_1px_var(--primary)] data-[state=checked]:bg-primary data-[state=checked]:text-background data-[state=checked]:shadow-none focus-visible:shadow-[0_0_0_2px_var(--ring)]";

const CHECKBOX_INDICATOR: &str = "flex items-center justify-center";

#[component]
pub fn Checkbox(props: CheckboxProps) -> Element {
    rsx! {
        checkbox::Checkbox {
            class: CHECKBOX,
            checked: props.checked,
            default_checked: props.default_checked,
            required: props.required,
            disabled: props.disabled,
            name: props.name,
            value: props.value,
            on_checked_change: props.on_checked_change,
            attributes: props.attributes,
            checkbox::CheckboxIndicator {
                class: CHECKBOX_INDICATOR,
                icon::Icon {
                    width: "1rem",
                    height: "1rem",
                    path { d: "M5 13l4 4L19 7" }
                }
            }
        }
    }
}
