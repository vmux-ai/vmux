use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const CARD_ROOT: &str = "flex flex-col gap-6 rounded-2xl border border-border bg-background py-6 text-muted-foreground shadow-[0_2px_10px_rgb(0_0_0_/_10%)] dark:border-muted dark:bg-card";

const CARD_HEADER: &str = "grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 [:has([data-slot=card-action])]:grid-cols-[1fr_auto]";

const CARD_TITLE: &str = "text-base font-semibold leading-none";

const CARD_DESCRIPTION: &str = "text-sm leading-5 text-muted-foreground";

const CARD_ACTION: &str = "col-start-2 row-span-2 row-start-1 justify-self-end";

const CARD_CONTENT: &str = "px-6";

const CARD_FOOTER: &str = "flex items-center px-6";

#[component]
pub fn Card(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: CARD_ROOT,
        "data-slot": "card",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
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
pub fn CardAction(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: CARD_ACTION,
        "data-slot": "card-action",
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

#[component]
pub fn CardFooter(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: CARD_FOOTER,
        "data-slot": "card-footer",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}
