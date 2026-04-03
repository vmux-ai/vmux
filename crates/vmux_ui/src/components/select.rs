use dioxus::prelude::*;
use dioxus_primitives::icon;
use dioxus_primitives::select::{
    self, SelectGroupLabelProps, SelectGroupProps, SelectListProps, SelectOptionProps, SelectProps,
    SelectTriggerProps, SelectValueProps,
};

const SELECT_ROOT: &str = "relative";

const SELECT_TRIGGER: &str = "relative box-border flex cursor-pointer flex-row items-center justify-between gap-1 rounded-md border-0 bg-background px-3 py-2 text-muted-foreground shadow-[inset_0_0_0_1px_var(--border)] transition-colors dark:bg-card dark:shadow-[inset_0_0_0_1px_var(--primary)] hover:bg-accent hover:text-foreground focus-visible:outline-none data-[disabled=true]:cursor-not-allowed";

const SELECT_LIST: &str = "absolute left-0 top-full z-[1000] mt-1 min-w-full origin-top rounded-lg border-0 bg-background p-1 opacity-0 shadow-[inset_0_0_0_1px_var(--border)] will-change-[transform,opacity] data-[state=closed]:pointer-events-none data-[state=closed]:animate-[dx-fade-zoom-out_150ms_ease-in_forwards] data-[state=open]:pointer-events-auto data-[state=open]:animate-[dx-fade-zoom-in_150ms_ease-out_forwards] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary)]";

const SELECT_GROUP_LABEL: &str = "px-3 py-1 text-xs text-muted-foreground";

const SELECT_OPTION: &str = "flex cursor-pointer items-center justify-between rounded-[calc(0.5rem-0.25rem)] px-3 py-2 text-sm hover:bg-accent hover:text-foreground focus-visible:outline-none data-[disabled=true]:cursor-not-allowed data-[disabled=true]:text-muted-foreground dark:hover:bg-primary dark:hover:text-foreground";

#[component]
pub fn Select<T: Clone + PartialEq + 'static>(props: SelectProps<T>) -> Element {
    rsx! {
        select::Select {
            class: SELECT_ROOT,
            value: props.value,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            disabled: props.disabled,
            name: props.name,
            placeholder: props.placeholder,
            roving_loop: props.roving_loop,
            typeahead_timeout: props.typeahead_timeout,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SelectTrigger(props: SelectTriggerProps) -> Element {
    rsx! {
        select::SelectTrigger {
            class: SELECT_TRIGGER,
            attributes: props.attributes,
            {props.children}
            icon::Icon {
                width: "20px",
                height: "20px",
                stroke: "var(--primary-color-7)",
                polyline { points: "6 9 12 15 18 9" }
            }
        }
    }
}

#[component]
pub fn SelectValue(props: SelectValueProps) -> Element {
    rsx! {
        select::SelectValue { attributes: props.attributes }
    }
}

#[component]
pub fn SelectList(props: SelectListProps) -> Element {
    rsx! {
        select::SelectList {
            class: SELECT_LIST,
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SelectGroup(props: SelectGroupProps) -> Element {
    rsx! {
        select::SelectGroup {
            disabled: props.disabled,
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SelectGroupLabel(props: SelectGroupLabelProps) -> Element {
    rsx! {
        select::SelectGroupLabel {
            class: SELECT_GROUP_LABEL,
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SelectOption<T: Clone + PartialEq + 'static>(props: SelectOptionProps<T>) -> Element {
    rsx! {
        select::SelectOption::<T> {
            class: SELECT_OPTION,
            value: props.value,
            text_value: props.text_value,
            disabled: props.disabled,
            id: props.id,
            index: props.index,
            aria_label: props.aria_label,
            aria_roledescription: props.aria_roledescription,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SelectItemIndicator() -> Element {
    rsx! {
        select::SelectItemIndicator {
            icon::Icon {
                width: "1rem",
                height: "1rem",
                stroke: "var(--secondary-color-5)",
                path { d: "M5 13l4 4L19 7" }
            }
        }
    }
}
