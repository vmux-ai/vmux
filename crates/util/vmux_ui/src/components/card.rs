use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

use crate::cn::cn;

const CARD_ROOT: &str =
    "flex flex-col gap-6 rounded-xl bg-foreground/[0.035] py-6 text-muted-foreground";
const PANEL_ROOT: &str = "flex min-h-0 flex-col overflow-hidden rounded-lg bg-foreground/[0.03] text-foreground transition-colors duration-150 hover:bg-foreground/[0.045]";

const CARD_HEADER: &str = "grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 [:has([data-slot=card-action])]:grid-cols-[1fr_auto]";

const CARD_TITLE: &str = "text-base font-semibold leading-none";

const CARD_DESCRIPTION: &str = "text-sm leading-5 text-muted-foreground";

const CARD_CONTENT: &str = "px-6";

#[derive(Clone, Copy, Default, PartialEq)]
pub enum CardVariant {
    #[default]
    Default,
    Panel,
}

impl CardVariant {
    fn classes(self) -> &'static str {
        match self {
            Self::Default => CARD_ROOT,
            Self::Panel => PANEL_ROOT,
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
        class: CARD_HEADER,
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
        class: CARD_TITLE,
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
        class: CARD_DESCRIPTION,
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
        class: CARD_CONTENT,
        "data-slot": "card-content",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}
