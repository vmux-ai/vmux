use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const PROMPT_BOX_ROOT: &str = "vmux-prompt-box relative overflow-hidden rounded-[1.25rem] bg-background/90 p-2 shadow-[0_20px_60px_-28px_rgba(0,0,0,0.58),0_6px_20px_-12px_rgba(0,0,0,0.22),inset_0_1px_0_rgba(255,255,255,0.4)] ring-1 ring-inset ring-foreground/[0.12] backdrop-blur-2xl backdrop-saturate-150 transition-[background-color,box-shadow,filter] duration-200 focus-within:bg-background/95 focus-within:shadow-[0_28px_80px_-32px_rgba(0,0,0,0.68),0_8px_24px_-14px_rgba(0,0,0,0.28),inset_0_1px_0_rgba(255,255,255,0.5)] dark:bg-[#171717]/90 dark:ring-white/[0.13] dark:focus-within:bg-[#171717]/95";

const PROMPT_POPUP_ROOT: &str = "vmux-prompt-popup absolute left-0 z-20 max-h-80 w-full overflow-x-hidden overflow-y-auto rounded-2xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PromptPopupPlacement {
    #[default]
    Upward,
    Downward,
    Inline,
}

/// Shared glass prompt surface used by launcher and agent composers.
#[component]
pub fn PromptBox(
    #[props(default = true)] glass: bool,
    #[props(default)] vertical: bool,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let layout = if vertical {
        "flex flex-col items-stretch"
    } else {
        "flex items-center"
    };
    let class = if glass {
        format!("{PROMPT_BOX_ROOT} {layout}")
    } else {
        layout.to_string()
    };
    let base = attributes!(div {
        class,
        "data-slot": "prompt-box",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged,
            if glass {
                div { class: "pointer-events-none absolute inset-px rounded-[1.2rem] bg-gradient-to-b from-white/[0.18] via-white/[0.025] to-transparent dark:from-white/[0.075]" }
                div { class: "pointer-events-none absolute -left-10 -top-16 h-28 w-80 rotate-[-5deg] rounded-full bg-white/[0.08] blur-3xl" }
            }
            {children}
        }
    }
}

/// Shared animated menu surface for prompt suggestions and selectors.
#[component]
pub fn PromptPopup(
    #[props(default)] placement: PromptPopupPlacement,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let class = match placement {
        PromptPopupPlacement::Upward => {
            format!("{PROMPT_POPUP_ROOT} vmux-prompt-popup-upward bottom-full mb-2")
        }
        PromptPopupPlacement::Downward => {
            format!("{PROMPT_POPUP_ROOT} vmux-prompt-popup-downward top-full mt-2")
        }
        PromptPopupPlacement::Inline => String::new(),
    };
    let base = attributes!(div {
        class,
        "data-slot": "prompt-popup",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged, {children} }
    }
}
