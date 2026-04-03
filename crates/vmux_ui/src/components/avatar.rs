use dioxus::prelude::*;
use dioxus_primitives::avatar::{self, AvatarFallbackProps, AvatarImageProps, AvatarState};

#[derive(Clone, Copy, PartialEq, Default)]
pub enum AvatarImageSize {
    #[default]
    Small,
    Medium,
    Large,
}

impl AvatarImageSize {
    fn tw(self) -> &'static str {
        match self {
            AvatarImageSize::Small => "size-8 text-sm",
            AvatarImageSize::Medium => "size-12 text-xl",
            AvatarImageSize::Large => "size-16 text-2xl",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum AvatarShape {
    #[default]
    Circle,
    Rounded,
}

impl AvatarShape {
    fn tw(self) -> &'static str {
        match self {
            AvatarShape::Circle => "rounded-full",
            AvatarShape::Rounded => "rounded-lg",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct AvatarProps {
    #[props(default)]
    pub on_load: Option<EventHandler<()>>,

    #[props(default)]
    pub on_error: Option<EventHandler<()>>,

    #[props(default)]
    pub on_state_change: Option<EventHandler<AvatarState>>,

    #[props(default)]
    pub size: AvatarImageSize,

    #[props(default)]
    pub shape: AvatarShape,

    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    pub children: Element,
}

#[component]
pub fn Avatar(props: AvatarProps) -> Element {
    let class = format!(
        "group/avatar relative inline-flex shrink-0 cursor-pointer items-center justify-center font-medium text-muted-foreground data-[state=empty]:bg-primary data-[state=loading]:animate-pulse {} {}",
        props.size.tw(),
        props.shape.tw()
    );
    rsx! {
        avatar::Avatar {
            class,
            on_load: props.on_load,
            on_error: props.on_error,
            on_state_change: props.on_state_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AvatarImage(props: AvatarImageProps) -> Element {
    rsx! {
        avatar::AvatarImage {
            class: "aspect-square h-full w-full",
            src: props.src,
            alt: props.alt,
            attributes: props.attributes,
        }
    }
}

#[component]
pub fn AvatarFallback(props: AvatarFallbackProps) -> Element {
    rsx! {
        avatar::AvatarFallback {
            class: "flex h-full w-full items-center justify-center bg-background text-2xl text-muted-foreground group-data-[state=error]/avatar:bg-card",
            attributes: props.attributes,
            {props.children}
        }
    }
}
