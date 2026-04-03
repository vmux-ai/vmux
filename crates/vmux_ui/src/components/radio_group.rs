use dioxus::prelude::*;
use dioxus_primitives::radio_group::{self, RadioGroupProps, RadioItemProps};

const RADIO_GROUP: &str = "flex flex-col gap-3";

const RADIO_ITEM: &str = "relative flex cursor-pointer flex-row items-center gap-3 border-0 bg-transparent p-0 text-left text-sm text-muted-foreground before:pointer-events-auto before:box-border before:block before:size-4 before:shrink-0 before:rounded-full before:bg-background before:shadow-[inset_0_0_0_1px_var(--border)] before:content-[''] before:cursor-pointer focus-visible:outline-none focus-visible:before:shadow-[0_0_0_2px_var(--ring)] data-[disabled=true]:opacity-50 data-[disabled=true]:before:cursor-not-allowed data-[state=checked]:before:border-[0.25rem] data-[state=checked]:before:border-solid data-[state=checked]:before:border-background data-[state=checked]:before:bg-muted-foreground data-[state=checked]:before:shadow-none dark:before:bg-card dark:before:shadow-[inset_0_0_0_1px_var(--primary)] dark:data-[state=checked]:before:border-card";

#[component]
pub fn RadioGroup(props: RadioGroupProps) -> Element {
    rsx! {
        radio_group::RadioGroup {
            class: RADIO_GROUP,
            value: props.value,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            disabled: props.disabled,
            required: props.required,
            name: props.name,
            horizontal: props.horizontal,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn RadioItem(props: RadioItemProps) -> Element {
    rsx! {
        radio_group::RadioItem {
            class: RADIO_ITEM,
            value: props.value,
            index: props.index,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}
