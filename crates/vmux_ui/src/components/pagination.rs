use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::icon;
use dioxus_primitives::merge_attributes;

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum PaginationLinkSize {
    #[default]
    Icon,
    Default,
}

impl PaginationLinkSize {
    pub fn class(&self) -> &'static str {
        match self {
            PaginationLinkSize::Icon => "icon",
            PaginationLinkSize::Default => "default",
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
#[non_exhaustive]
pub enum PaginationLinkKind {
    Previous,
    Next,
}

impl PaginationLinkKind {
    pub fn attr(&self) -> &'static str {
        match self {
            PaginationLinkKind::Previous => "previous",
            PaginationLinkKind::Next => "next",
        }
    }
}

const PAGINATION_NAV: &str = "mx-auto flex w-full justify-center";

const PAGINATION_CONTENT: &str = "m-0 flex list-none items-center gap-1 p-0";

const PAGINATION_ITEM: &str = "inline-flex";

const PAGINATION_LINK: &str = "inline-flex box-border items-center justify-center gap-2 rounded-lg text-sm font-medium leading-none text-muted-foreground no-underline transition-colors focus-visible:shadow-[0_0_0_2px_var(--ring)] data-[active=true]:border data-[active=true]:border-border data-[active=true]:bg-background data-[active=true]:hover:bg-accent data-[active=false]:hover:bg-muted data-[active=false]:hover:text-foreground data-[size=icon]:size-8 data-[size=icon]:p-0 data-[size=default]:h-8 data-[size=default]:px-4 data-[size=default]:py-2 data-[kind=previous]:gap-1 data-[kind=previous]:pl-2.5 data-[kind=previous]:pr-2.5 data-[kind=next]:gap-1 data-[kind=next]:pl-2.5 data-[kind=next]:pr-2.5 dark:data-[active=true]:bg-card";

const PAGINATION_LABEL: &str = "hidden sm:inline";

const PAGINATION_ELLIPSIS: &str = "flex size-8 items-center justify-center text-muted-foreground";

#[component]
pub fn Pagination(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(nav {
        class: PAGINATION_NAV,
        "data-slot": "pagination",
        role: "navigation",
        aria_label: "pagination",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        nav { ..merged, {children} }
    }
}

#[component]
pub fn PaginationContent(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(ul {
        class: PAGINATION_CONTENT,
        "data-slot": "pagination-content",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        ul { ..merged, {children} }
    }
}

#[component]
pub fn PaginationItem(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(li {
        class: PAGINATION_ITEM,
        "data-slot": "pagination-item",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        li { ..merged, {children} }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PaginationLinkProps {
    #[props(default)]
    pub is_active: bool,
    #[props(default)]
    pub size: PaginationLinkSize,
    #[props(default)]
    pub data_kind: Option<PaginationLinkKind>,
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = a)]
    pub attributes: Vec<Attribute>,
    pub children: Element,
}

#[component]
pub fn PaginationLink(props: PaginationLinkProps) -> Element {
    let aria_current = if props.is_active { Some("page") } else { None };
    let data_kind = props.data_kind.map(|kind| kind.attr());
    let base = attributes!(a {
        class: PAGINATION_LINK,
        "data-slot": "pagination-link",
        "data-active": props.is_active,
        "data-size": props.size.class(),
        "data-kind": data_kind,
        aria_current: aria_current,
    });
    let merged = merge_attributes(vec![base, props.attributes.clone()]);
    rsx! {
        a {
            onclick: move |event| {
                if let Some(f) = &props.onclick {
                    f.call(event);
                }
            },
            onmousedown: move |event| {
                if let Some(f) = &props.onmousedown {
                    f.call(event);
                }
            },
            onmouseup: move |event| {
                if let Some(f) = &props.onmouseup {
                    f.call(event);
                }
            },
            ..merged,
            {props.children}
        }
    }
}

#[component]
pub fn PaginationPrevious(
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = a)]
    attributes: Vec<Attribute>,
) -> Element {
    rsx! {
        PaginationLink {
            size: PaginationLinkSize::Default,
            aria_label: "Go to previous page",
            data_kind: Some(PaginationLinkKind::Previous),
            onclick,
            onmousedown,
            onmouseup,
            attributes,
            icon::Icon {
                width: "1rem",
                height: "1rem",
                polyline { points: "15 6 9 12 15 18" }
            }
            span { class: PAGINATION_LABEL, "Previous" }
        }
    }
}

#[component]
pub fn PaginationNext(
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = a)]
    attributes: Vec<Attribute>,
) -> Element {
    rsx! {
        PaginationLink {
            size: PaginationLinkSize::Default,
            aria_label: "Go to next page",
            data_kind: Some(PaginationLinkKind::Next),
            onclick,
            onmousedown,
            onmouseup,
            attributes,
            span { class: PAGINATION_LABEL, "Next" }
            icon::Icon {
                width: "1rem",
                height: "1rem",
                polyline { points: "9 6 15 12 9 18" }
            }
        }
    }
}

#[component]
pub fn PaginationEllipsis(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(span {
        class: PAGINATION_ELLIPSIS,
        "data-slot": "pagination-ellipsis",
        aria_hidden: "true",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        span {
            ..merged,
            icon::Icon {
                width: "1rem",
                height: "1rem",
                fill: "currentColor",
                circle { cx: "5", cy: "12", r: "1.5" }
                circle { cx: "12", cy: "12", r: "1.5" }
                circle { cx: "19", cy: "12", r: "1.5" }
            }
            span { class: "sr-only", "More pages" }
        }
    }
}
