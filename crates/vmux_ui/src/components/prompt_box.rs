use crate::components::button::IconButton;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const PROMPT_BOX_ROOT: &str = "vmux-prompt-box relative z-20 flex items-center overflow-hidden rounded-2xl bg-white/45 p-1 shadow-[0_18px_55px_-24px_rgba(0,0,0,0.65),inset_0_1px_0_rgba(255,255,255,0.18),inset_0_-1px_0_rgba(255,255,255,0.04)] ring-1 ring-inset ring-black/10 backdrop-blur-3xl backdrop-saturate-150 transition-all duration-200 focus-within:bg-white/55 focus-within:ring-black/20 focus-within:shadow-[0_22px_65px_-24px_rgba(0,0,0,0.72),inset_0_1px_0_rgba(255,255,255,0.22)] dark:bg-white/[0.045] dark:ring-white/[0.16] dark:focus-within:bg-white/[0.065] dark:focus-within:ring-white/25";
const PROMPT_POPUP_ROOT: &str = "vmux-prompt-popup absolute left-0 z-20 max-h-80 w-full overflow-x-hidden overflow-y-auto rounded-2xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl";

const PROMPT_POPUP_HEADER: &str =
    "pointer-events-none sticky top-0 z-10 flex items-center justify-end";
const PROMPT_POPUP_HEADING: &str = "mr-auto truncate px-3 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60";

pub const PROMPT_MENU_ROW: &str =
    "flex w-full min-h-9 items-center gap-2 px-3 py-1.5 text-left text-sm";
pub const PROMPT_MENU_ROW_IDLE: &str = "transition hover:bg-foreground/[0.06]";
pub const PROMPT_MENU_ROW_SELECTED: &str = "bg-foreground/[0.08]";
pub const PROMPT_MENU_INDENT: &str = "pl-8";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PromptPopupPlacement {
    #[default]
    Upward,
    Downward,
    Inline,
}

#[component]
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
pub fn PromptPopup(
    #[props(default)] placement: PromptPopupPlacement,
    #[props(default)] heading: Option<String>,
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
    let header_class = match heading.is_some() {
        true => format!("{PROMPT_POPUP_HEADER} bg-background/95 backdrop-blur"),
        false => PROMPT_POPUP_HEADER.to_string(),
    };
    let has_header = heading.is_some() || on_dismiss.is_some();
    rsx! {
        if let Some(on_dismiss) = on_dismiss {
            div {
                class: "fixed inset-0 z-10",
                onmousedown: move |event: MouseEvent| {
                    event.prevent_default();
                    on_dismiss.call(());
                },
            }
        }
        div { ..merged,
            if has_header {
                div { class: "{header_class}",
                    if let Some(heading) = heading {
                        span { class: PROMPT_POPUP_HEADING, "{heading}" }
                    }
                    if let Some(on_dismiss) = on_dismiss {
                        IconButton {
                            class: "pointer-events-auto m-1 bg-background/80 backdrop-blur",
                            label: crate::i18n::translate("common-close"),
                            paths: vec!["M18 6 6 18".to_string(), "m6 6 12 12".to_string()],
                            onmousedown: move |event: MouseEvent| event.prevent_default(),
                            onclick: move |_| on_dismiss.call(()),
                        }
                    }
                }
            }
            {children}
        }
    }
}
