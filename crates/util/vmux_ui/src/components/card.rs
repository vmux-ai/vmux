use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

use crate::cn::cn;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum CardVariant {
    #[default]
    Default,
    Panel,
}

impl CardVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Default => {
                "flex flex-col gap-6 rounded-xl bg-foreground/[0.035] py-6 text-muted-foreground"
            }
            Self::Panel => {
                "flex min-h-0 flex-col overflow-hidden rounded-lg bg-foreground/[0.03] text-foreground transition-colors duration-150 hover:bg-foreground/[0.045]"
            }
        }
    }
}

#[component]
pub fn Card(
    #[props(default)] variant: CardVariant,
    onclick: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: cn([variant.classes()]),
        "data-slot": "card",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div {
            onclick: move |event| _ = onclick.map(|callback| callback(event)),
            ..merged,
            {children}
        }
    }
}

#[component]
pub fn CardHeader(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 [:has([data-slot=card-action])]:grid-cols-[1fr_auto]",
        "data-slot": "card-header",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}

#[component]
pub fn CardTitle(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "text-base font-semibold leading-none",
        "data-slot": "card-title",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}

#[component]
pub fn CardDescription(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "text-sm leading-5 text-muted-foreground",
        "data-slot": "card-description",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}

#[component]
pub fn CardContent(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "px-6",
        "data-slot": "card-content",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}
