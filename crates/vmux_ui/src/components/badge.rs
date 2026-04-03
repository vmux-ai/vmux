use dioxus::prelude::*;
use dioxus_primitives::icon;

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum BadgeVariant {
    #[default]
    Primary,
    Secondary,
    Destructive,
    Outline,
}

impl BadgeVariant {
    pub fn classes(&self) -> &'static str {
        match self {
            BadgeVariant::Primary => {
                "inline-flex min-h-5 min-w-[20px] items-center justify-center gap-1 rounded-[10px] px-2 text-xs shadow-[0_0_0_1px_var(--background)] bg-primary text-primary-foreground"
            }
            BadgeVariant::Secondary => {
                "inline-flex min-h-5 min-w-[20px] items-center justify-center gap-1 rounded-[10px] px-2 text-xs shadow-[0_0_0_1px_var(--background)] bg-secondary text-secondary-foreground"
            }
            BadgeVariant::Destructive => {
                "inline-flex min-h-5 min-w-[20px] items-center justify-center gap-1 rounded-[10px] px-2 text-xs shadow-[0_0_0_1px_var(--background)] bg-destructive text-primary-foreground"
            }
            BadgeVariant::Outline => {
                "inline-flex min-h-5 min-w-[20px] items-center justify-center gap-1 rounded-[10px] border border-border bg-background px-2 text-xs text-muted-foreground"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BadgeProps {
    #[props(default)]
    pub variant: BadgeVariant,

    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    pub children: Element,
}

#[component]
pub fn Badge(props: BadgeProps) -> Element {
    rsx! {
        BadgeElement {
            variant: props.variant,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
fn BadgeElement(props: BadgeProps) -> Element {
    rsx! {
        span {
            class: props.variant.classes(),
            ..props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn VerifiedIcon() -> Element {
    rsx! {
        icon::Icon {
            class: "text-muted-foreground",
            width: "12px",
            height: "12px",
            stroke: "currentColor",
            path { d: "M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1-4.78-4.77 4 4 0 0 1 0-6.76Z" }
            path { d: "m9 12 2 2 4-4" }
        }
    }
}
