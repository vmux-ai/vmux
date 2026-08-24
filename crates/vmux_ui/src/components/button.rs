use crate::components::icon::Icon;
use crate::util::merge_class;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const BUTTON_BASE: &str = "cursor-pointer inline-flex items-center justify-center rounded-lg font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50";

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Destructive,
    Outline,
    Ghost,
}

impl ButtonVariant {
    pub fn classes(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "bg-primary text-primary-foreground hover:bg-primary/90",
            ButtonVariant::Secondary => {
                "bg-secondary text-secondary-foreground hover:bg-secondary/80"
            }
            ButtonVariant::Destructive => {
                "bg-destructive text-primary-foreground hover:bg-destructive/90"
            }
            ButtonVariant::Outline => {
                "border border-input bg-background text-foreground hover:bg-accent hover:text-accent-foreground"
            }
            ButtonVariant::Ghost => {
                "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum ButtonSize {
    Xs,
    Sm,
    #[default]
    Md,
    Icon,
    Block,
}

impl ButtonSize {
    pub fn classes(&self) -> &'static str {
        match self {
            ButtonSize::Xs => "gap-1 px-2 py-0.5 text-xs",
            ButtonSize::Sm => "gap-1.5 px-3 py-1 text-sm",
            ButtonSize::Md => "gap-2 px-[18px] py-2 text-base",
            ButtonSize::Icon => "h-7 w-7 shrink-0",
            ButtonSize::Block => "w-full justify-start gap-2 px-3 py-1.5 text-left text-sm",
        }
    }
}

#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(extends=GlobalAttributes)]
    #[props(extends=button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let class = merge_class(
        BUTTON_BASE,
        Some(&merge_class(size.classes(), Some(variant.classes()))),
    );
    let base = attributes!(button { class });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
        button {
            onclick: move |event| {
                if let Some(f) = &onclick {
                    f.call(event);
                }
            },
            onmousedown: move |event| {
                if let Some(f) = &onmousedown {
                    f.call(event);
                }
            },
            onmouseup: move |event| {
                if let Some(f) = &onmouseup {
                    f.call(event);
                }
            },
            ..merged,
            {children}
        }
    }
}

#[component]
pub fn IconButton(
    label: String,
    paths: Vec<String>,
    #[props(default = ButtonVariant::Ghost)] variant: ButtonVariant,
    #[props(extends=GlobalAttributes)]
    #[props(extends=button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    onmousedown: Option<EventHandler<MouseEvent>>,
    onmouseup: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! {
        Button {
            variant,
            size: ButtonSize::Icon,
            aria_label: "{label}",
            onclick,
            onmousedown,
            onmouseup,
            attributes,
            Icon { class: "h-4 w-4",
                for d in paths {
                    path { key: "{d}", d: "{d}" }
                }
            }
        }
    }
}
