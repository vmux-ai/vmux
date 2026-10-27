use crate::cn::cn;
use crate::components::button::IconButton;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

const PROMPT_BOX_ROOT: &str = "vmux-prompt-box relative z-20 flex items-center rounded-xl bg-foreground/[0.055] p-1 ring-1 ring-inset ring-foreground/[0.08] transition-colors duration-150 focus-within:bg-foreground/[0.075] focus-within:ring-foreground/[0.14]";
const PROMPT_POPUP_ROOT: &str = "vmux-prompt-popup absolute left-0 z-20 max-h-80 w-full overflow-x-hidden overflow-y-auto rounded-xl bg-card shadow-lg ring-1 ring-inset ring-foreground/[0.08]";

const PROMPT_POPUP_HEADER: &str =
    "pointer-events-none sticky top-0 z-10 flex items-center justify-end";
const PROMPT_POPUP_HEADING: &str = "mr-auto truncate px-3 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60";

pub const PROMPT_MENU_ROW: &str =
    "flex w-full min-h-9 items-center gap-2 px-3 py-1.5 text-left text-sm";
pub const PROMPT_MENU_ROW_IDLE: &str = "transition hover:bg-foreground/[0.06]";
pub const PROMPT_MENU_ROW_SELECTED: &str = "bg-foreground/[0.08]";
pub const PROMPT_MENU_INDENT: &str = "pl-8";

pub struct PromptMenuRow;

impl PromptMenuRow {
    const CURSOR: &'static str = "bg-primary/12 shadow-[inset_2px_0_0_0_var(--primary)]";

    pub fn class(at_cursor: bool) -> String {
        match at_cursor {
            true => cn([PROMPT_MENU_ROW, Self::CURSOR]),
            false => cn([PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE]),
        }
    }
}

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
        PromptPopupPlacement::Upward => cn([
            root,
            "bottom-full mb-2 origin-bottom animate-prompt-popup-upward motion-reduce:animate-none",
        ]),
        PromptPopupPlacement::Downward => cn([
            root,
            "top-full mt-2 origin-top animate-prompt-popup-downward motion-reduce:animate-none",
        ]),
        PromptPopupPlacement::Inline => String::new(),
    };
    let base = attributes!(div {
        class,
        "data-slot": "prompt-popup",
    });
    let merged = merge_attributes(vec![base, attributes]);
    let header_class = cn([
        PROMPT_POPUP_HEADER,
        if heading.is_some() { "bg-card" } else { "" },
    ]);
    let has_header = heading.is_some();
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
                            class: "pointer-events-auto m-1 bg-foreground/[0.05]",
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
