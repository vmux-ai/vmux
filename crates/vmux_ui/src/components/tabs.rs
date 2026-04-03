use dioxus::prelude::*;
use dioxus_primitives::tabs::{self, TabContentProps, TabListProps, TabTriggerProps};

use crate::util::merge_class;

/// The props for the [`Tabs`] component.
#[derive(Props, Clone, PartialEq)]
pub struct TabsProps {
    /// The class of the tabs component.
    #[props(default)]
    pub class: String,

    /// The controlled value of the active tab.
    pub value: ReadSignal<Option<String>>,

    /// The default active tab value when uncontrolled.
    #[props(default)]
    pub default_value: String,

    /// Callback fired when the active tab changes.
    #[props(default)]
    pub on_value_change: Callback<String>,

    /// Whether the tabs are disabled.
    #[props(default)]
    pub disabled: ReadSignal<bool>,

    /// Whether the tabs are horizontal.
    #[props(default)]
    pub horizontal: ReadSignal<bool>,

    /// Whether focus should loop around when reaching the end.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub roving_loop: ReadSignal<bool>,

    /// The variant of the tabs component.
    #[props(default)]
    pub variant: TabsVariant,

    /// Additional attributes to apply to the tabs element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the tabs component.
    pub children: Element,
}

/// The variant of the tabs component.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum TabsVariant {
    /// The default variant.
    #[default]
    Default,
    /// The ghost variant.
    Ghost,
}

impl TabsVariant {
    /// Convert the variant to a string for use in class names
    fn to_class(self) -> &'static str {
        match self {
            TabsVariant::Default => "default",
            TabsVariant::Ghost => "ghost",
        }
    }
}

#[component]
pub fn Tabs(props: TabsProps) -> Element {
    rsx! {
        tabs::Tabs {
            class: merge_class(
                "group/tabs flex w-full flex-col gap-2",
                Some((props.class + " ").trim()),
            ),
            "data-variant": props.variant.to_class(),
            value: props.value,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            disabled: props.disabled,
            horizontal: props.horizontal,
            roving_loop: props.roving_loop,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn TabList(props: TabListProps) -> Element {
    rsx! {
        tabs::TabList {
            class: "flex w-fit flex-1 flex-row gap-1 rounded-lg border-none p-1 group-data-[variant=default]/tabs:bg-card dark:group-data-[variant=default]/tabs:bg-muted",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn TabTrigger(props: TabTriggerProps) -> Element {
    rsx! {
        tabs::TabTrigger {
            class: "cursor-pointer rounded-[calc(0.5rem-0.25rem)] border-0 bg-transparent px-2 py-1 text-muted-foreground data-[disabled=true]:cursor-not-allowed data-[state=active]:text-foreground group-data-[variant=default]/tabs:data-[state=active]:bg-background group-data-[variant=default]/tabs:data-[state=active]:shadow-sm dark:group-data-[variant=default]/tabs:data-[state=active]:shadow-[inset_0_0_0_1px_var(--primary)] hover:text-secondary focus-visible:text-secondary",
            id: props.id,
            value: props.value,
            index: props.index,
            disabled: props.disabled,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn TabContent(props: TabContentProps) -> Element {
    let class = merge_class(
        "w-full box-border p-1 data-[state=inactive]:hidden group-data-[variant=default]/tabs:rounded-lg group-data-[variant=default]/tabs:border group-data-[variant=default]/tabs:border-border group-data-[variant=default]/tabs:bg-background group-data-[variant=default]/tabs:shadow-sm dark:group-data-[variant=default]/tabs:border-primary dark:group-data-[variant=default]/tabs:bg-card",
        props.class.as_deref(),
    );
    rsx! {
        tabs::TabContent {
            class,
            value: props.value,
            id: props.id,
            index: props.index,
            attributes: props.attributes,
            {props.children}
        }
    }
}
