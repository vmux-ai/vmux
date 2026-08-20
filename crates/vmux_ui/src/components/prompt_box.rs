use crate::components::button::IconButton;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const PROMPT_BOX_ROOT: &str = "vmux-prompt-box relative flex items-center overflow-hidden rounded-2xl bg-white/45 p-1 shadow-[0_18px_55px_-24px_rgba(0,0,0,0.65),inset_0_1px_0_rgba(255,255,255,0.18),inset_0_-1px_0_rgba(255,255,255,0.04)] ring-1 ring-inset ring-black/10 backdrop-blur-3xl backdrop-saturate-150 transition-all duration-200 focus-within:bg-white/55 focus-within:ring-black/20 focus-within:shadow-[0_22px_65px_-24px_rgba(0,0,0,0.72),inset_0_1px_0_rgba(255,255,255,0.22)] dark:bg-white/[0.045] dark:ring-white/[0.16] dark:focus-within:bg-white/[0.065] dark:focus-within:ring-white/25";
const PROMPT_POPUP_ROOT: &str = "vmux-prompt-popup absolute left-0 z-20 max-h-80 w-full overflow-x-hidden overflow-y-auto rounded-2xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// Placement of a prompt-related popup relative to its composer.
pub enum PromptPopupPlacement {
    #[default]
    Upward,
    Downward,
    Inline,
}

#[component]
/// Shared glass prompt surface used by desktop and mobile composers.
pub fn PromptBox(
    #[props(default = true)] glass: bool,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let class = if glass { PROMPT_BOX_ROOT } else { "" };
    let base = attributes!(div {
        class,
        "data-slot": "prompt-box",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged,
            if glass {
                div { class: "pointer-events-none absolute inset-px rounded-[0.9rem] bg-gradient-to-b from-white/[0.12] via-white/[0.025] to-transparent dark:from-white/[0.10]" }
                div { class: "pointer-events-none absolute -left-12 -top-12 h-24 w-72 rotate-[-5deg] rounded-full bg-white/[0.09] blur-2xl" }
            }
            {children}
        }
    }
}

#[component]
/// Shared popup surface for prompt suggestions and selectors.
///
/// A popup opened by what was typed is closed by Escape, which a software keyboard does not have.
/// Passing `on_dismiss` adds the only other way out that does not involve deleting the draft a
/// character at a time; it is a plain button, so it serves a mouse as readily as a thumb.
pub fn PromptPopup(
    #[props(default)] placement: PromptPopupPlacement,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let root = PROMPT_POPUP_ROOT;
    let class = match placement {
        PromptPopupPlacement::Upward => {
            format!("{root} vmux-prompt-popup-upward bottom-full mb-2")
        }
        PromptPopupPlacement::Downward => {
            format!("{root} vmux-prompt-popup-downward top-full mt-2")
        }
        PromptPopupPlacement::Inline => String::new(),
    };
    let base = attributes!(div {
        class,
        "data-slot": "prompt-popup",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div { ..merged,
            if let Some(on_dismiss) = on_dismiss {
                // Sticky rather than floating: the list scrolls under it, so it cannot be scrolled
                // out of reach in a menu long enough to need scrolling in the first place.
                div { class: "pointer-events-none sticky top-0 z-10 flex justify-end",
                    IconButton {
                        class: "pointer-events-auto m-1 bg-background/80 backdrop-blur",
                        label: crate::i18n::translate("common-close"),
                        paths: vec!["M18 6 6 18".to_string(), "m6 6 12 12".to_string()],
                        // The prompt keeps focus, so the menu does not reopen from a refocus and
                        // the keyboard does not drop on the way out.
                        onmousedown: move |event: MouseEvent| event.prevent_default(),
                        onclick: move |_| on_dismiss.call(()),
                    }
                }
            }
            {children}
        }
    }
}
