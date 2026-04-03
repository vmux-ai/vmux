use dioxus::prelude::*;
use dioxus_primitives::toggle_group::{self, ToggleGroupProps, ToggleItemProps};

const TOGGLE_GROUP: &str = "group w-fit";

const TOGGLE_ITEM: &str = "min-w-[35px] rounded-none border-0 bg-transparent p-2.5 text-sm text-muted-foreground outline-none transition-colors first:rounded-l-lg last:rounded-r-lg hover:cursor-pointer hover:bg-accent focus-visible:cursor-pointer focus-visible:bg-accent data-[state=on]:bg-primary data-[state=on]:text-foreground group-data-[allow-multiple-pressed=true]:border-y group-data-[allow-multiple-pressed=true]:border-r group-data-[allow-multiple-pressed=true]:border-border group-data-[allow-multiple-pressed=true]:first:border-l group-data-[allow-multiple-pressed=true]:first:data-[state=on]:border group-data-[allow-multiple-pressed=true]:data-[state=on]:border-y group-data-[allow-multiple-pressed=true]:data-[state=on]:border-r";

#[component]
pub fn ToggleGroup(props: ToggleGroupProps) -> Element {
    rsx! {
        toggle_group::ToggleGroup {
            class: TOGGLE_GROUP,
            default_pressed: props.default_pressed,
            pressed: props.pressed,
            on_pressed_change: props.on_pressed_change,
            disabled: props.disabled,
            allow_multiple_pressed: props.allow_multiple_pressed,
            horizontal: props.horizontal,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ToggleItem(props: ToggleItemProps) -> Element {
    rsx! {
        toggle_group::ToggleItem {
            class: TOGGLE_ITEM,
            index: props.index,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}
