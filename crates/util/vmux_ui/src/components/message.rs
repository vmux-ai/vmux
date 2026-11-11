use crate::components::avatar::IdentityOrb;
use dioxus::prelude::*;

#[component]
pub fn MessageFrame(
    name: String,
    #[props(default)] avatar: Option<Element>,
    #[props(default)] seed: String,
    #[props(default)] metadata: Option<Element>,
    #[props(default)] compact: bool,
    #[props(default = true)] reserve_metadata: bool,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let seed = if seed.is_empty() { name.clone() } else { seed };
    let presentation = if compact {
        ""
    } else {
        "chat-message-enter [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]"
    };
    rsx! {
        div {
            class: "{presentation} chat-assistant-turn group flex w-full gap-3 px-1 py-1",
            "data-vmux-scroll-layout": (!compact).then_some(""),
            ..attributes,
            if let Some(avatar) = avatar {
                {avatar}
            } else {
                IdentityOrb {
                    seed,
                    alt: name.clone(),
                    class: "mt-0.5 size-6 shrink-0".to_string(),
                }
            }
            div { class: "relative min-w-0 flex-1",
                if !compact { div { class: "mb-1 text-xs font-medium text-muted-foreground", "{name}" } }
                div { class: "flex min-w-0 flex-col gap-2.5", {children} }
                if let Some(metadata) = metadata {
                    {metadata}
                } else if reserve_metadata && !compact {
                    div { class: "mt-1 h-6" }
                }
            }
        }
    }
}

pub struct WelcomeLayout;

impl WelcomeLayout {
    pub const GUIDE_HEIGHT: u32 = 96;
    pub const GAP: u32 = 16;
    pub const COMPOSER_HEIGHT: u32 = 114;
    pub const HEIGHT: u32 = Self::GUIDE_HEIGHT + Self::GAP + Self::COMPOSER_HEIGHT;

    pub fn dock_offset() -> u32 {
        Self::HEIGHT / 2 + 12
    }

    pub fn style() -> String {
        format!(
            "--vmux-welcome-height:{}px;--vmux-welcome-guide-height:{}px;--vmux-welcome-composer-height:{}px;--vmux-welcome-gap:{}px;",
            Self::HEIGHT,
            Self::GUIDE_HEIGHT,
            Self::COMPOSER_HEIGHT,
            Self::GAP,
        )
    }
}
