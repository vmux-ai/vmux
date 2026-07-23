use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::translate;
use dioxus::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::prompt_ghost::PromptGhost;

use super::prompt_box::PromptBox;

pub const PROMPT_INPUT_ID: &str = "vmux-prompt-input";

const PROMPT_COMPOSER_CSS: &str = r#"
.vmux-prompt-input{caret-color:var(--vmux-prompt-accent)}
.vmux-prompt-composer{box-shadow:0 22px 70px -30px rgba(0,0,0,.58),0 8px 24px -16px rgba(0,0,0,.3),inset 0 1px 0 rgba(255,255,255,.16)}
.vmux-prompt-composer:focus-within{box-shadow:0 28px 84px -34px rgba(0,0,0,.7),0 10px 28px -18px color-mix(in srgb,var(--vmux-prompt-accent) 32%,transparent),inset 0 0 0 1px color-mix(in srgb,var(--vmux-prompt-accent) 28%,transparent),inset 0 1px 0 rgba(255,255,255,.2)}
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptComposerAttachment {
    pub key: String,
    pub name: String,
    pub label: String,
    pub preview_data_url: String,
    pub remove_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptComposerAction {
    #[default]
    Send,
    Stop,
}

#[component]
pub fn PromptComposer(
    value: String,
    #[props(default)] preview: String,
    #[props(default)] completion: String,
    #[props(default)] attachments: Vec<PromptComposerAttachment>,
    #[props(default)] show_examples: bool,
    placeholder: String,
    accent_bg: String,
    accent_color: String,
    accent_gradient: String,
    #[props(default)] footer: Option<Element>,
    #[props(default)] action: PromptComposerAction,
    action_title: String,
    action_enabled: bool,
    on_input: EventHandler<String>,
    on_keydown: EventHandler<KeyboardEvent>,
    on_paste: EventHandler<()>,
    on_attach: EventHandler<()>,
    on_remove_attachment: EventHandler<usize>,
    on_action: EventHandler<()>,
) -> Element {
    use_effect(use_reactive((&value,), move |_| {
        resize_prompt_textarea(PROMPT_INPUT_ID);
    }));

    let action_class = if action_enabled {
        match action {
            PromptComposerAction::Send => format!(
                "relative z-10 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-gradient-to-br text-white shadow-[0_5px_14px_-5px_rgba(0,0,0,0.55)] transition hover:scale-[1.04] hover:brightness-110 active:scale-95 {accent_gradient}"
            ),
            PromptComposerAction::Stop => "relative z-10 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-foreground/[0.09] text-foreground/70 shadow-sm ring-1 ring-inset ring-foreground/10 transition hover:bg-foreground/[0.14] hover:text-foreground active:scale-95".to_string(),
        }
    } else {
        "relative z-10 flex h-8 w-8 shrink-0 cursor-default items-center justify-center rounded-full bg-foreground/[0.055] text-muted-foreground/35 ring-1 ring-inset ring-foreground/[0.07]".to_string()
    };

    rsx! {
            style { dangerous_inner_html: PROMPT_COMPOSER_CSS }
            PromptBox {
                vertical: true,
                class: "vmux-prompt-composer",
                style: "--vmux-prompt-accent:{accent_color};",
                div { class: "relative z-10 px-2 pt-1",
                    if !attachments.is_empty() {
                        div { class: "mb-1.5 flex flex-wrap items-center gap-1.5",
                    for attachment in attachments.iter().cloned() {
                        div {
                            key: "{attachment.key}",
                            class: if attachment.remove_index.is_some() { "group flex h-7 max-w-56 shrink-0 items-center gap-1.5 rounded-lg bg-foreground/[0.065] pl-1 pr-1 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/[0.08]" } else { "flex h-7 max-w-56 shrink-0 items-center gap-1.5 rounded-lg bg-foreground/[0.065] pl-1 pr-2 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/[0.08]" },
                            if attachment.preview_data_url.is_empty() {
                                span { class: "flex h-5 min-w-5 items-center justify-center rounded-full bg-foreground/[0.08] px-1 font-mono text-[8px] font-semibold text-muted-foreground",
                                    "{attachment.label}"
                                }
                            } else {
                                img {
                                    src: "{attachment.preview_data_url}",
                                    alt: "{attachment.name}",
                                    class: "h-5 w-5 rounded-full object-cover",
                                }
                            }
                            span { class: "min-w-0 max-w-40 truncate", "{attachment.name}" }
                            if let Some(remove_index) = attachment.remove_index {
                                button {
                                    class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-foreground/45 transition hover:bg-foreground/10 hover:text-foreground",
                                    title: translate("composer-remove-attachment"),
                                    onmousedown: move |event| event.prevent_default(),
                                    onclick: move |_| {
                                        on_remove_attachment.call(remove_index);
                                        focus_prompt_end(PROMPT_INPUT_ID);
                                    },
                                    svg {
                                        class: "h-3 w-3",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2.5",
                                        stroke_linecap: "round",
                                        path { d: "M6 6l12 12M18 6L6 18" }
                                    }
                                }
                            }
                        }
                    }
                        }
                    }
                    div { class: "relative min-w-32 overflow-hidden",
                        if value.is_empty() {
                            div { class: "pointer-events-none absolute inset-0 flex -translate-y-px items-center overflow-hidden px-1 py-1",
                                if !preview.is_empty() {
                                    div { class: "max-w-full truncate whitespace-nowrap text-[15px] leading-6 text-foreground", "{preview}" }
                                } else if show_examples {
                                    PromptGhost {
                                        accent_bg,
                                        terminal: false,
                                    }
                                } else {
                                    div { class: "flex max-w-full items-center whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50",
                                        span { class: "min-w-0 truncate", "{placeholder}" }
                                    }
                                }
                            }
                        }
                        if !completion.is_empty() {
                            div {
                                class: "pointer-events-none absolute inset-0 overflow-hidden whitespace-pre-wrap break-words px-1 py-2 text-[15px] leading-6",
                                span { class: "text-transparent", "{value}" }
                                span { class: "text-muted-foreground/40", "{completion}" }
                            }
                        }
                        textarea {
                            id: PROMPT_INPUT_ID,
                            class: "vmux-prompt-input relative z-10 max-h-48 min-h-12 w-full resize-none bg-transparent px-1 py-2 text-[15px] leading-6 placeholder:text-transparent focus:outline-none",
                            autofocus: true,
                            rows: "1",
                            placeholder: "{placeholder}",
                            value: "{value}",
                            oninput: move |event| on_input.call(event.value()),
                            onpaste: move |_| on_paste.call(()),
                            onkeydown: move |event| on_keydown.call(event),
                        }
                    }
                }
                div { class: "relative z-10 mt-0.5 flex min-w-0 items-center gap-1 px-1",
                    button {
                        class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-foreground/50 transition hover:bg-foreground/[0.08] hover:text-foreground active:scale-95",
                        title: translate("composer-attach-files"),
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| on_attach.call(()),
                        svg {
                            class: "h-4 w-4",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            path { d: "M12 5v14M5 12h14" }
                        }
                    }
                    if let Some(footer) = footer {
                        div { class: "min-w-0 flex-1 overflow-hidden", {footer} }
                    } else {
                        div { class: "min-w-0 flex-1 truncate px-1 text-[10px] text-muted-foreground/55",
                            {translate("command-send")}
                        }
                    }
                    button {
                    class: "{action_class}",
                    disabled: !action_enabled,
                    title: "{action_title}",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        if action_enabled {
                            on_action.call(());
                        }
                    },
                    if action == PromptComposerAction::Stop {
                        svg {
                            class: "h-4 w-4",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            rect { x: "6", y: "6", width: "12", height: "12", rx: "2.5" }
                        }
                    } else {
                        svg {
                            class: "h-4 w-4",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M12 19V5" }
                            path { d: "M5 12l7-7 7 7" }
                        }
                    }
                }
            }
        }
    }
}

pub fn prompt_textarea(input_id: &str) -> Option<web_sys::HtmlTextAreaElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(input_id)?
        .dyn_into()
        .ok()
}

pub fn focus_prompt_end(input_id: &str) {
    let input_id = input_id.to_string();
    let closure = Closure::once(move || {
        let Some(textarea) = prompt_textarea(&input_id) else {
            return;
        };
        let end = textarea.value().encode_utf16().count() as u32;
        let _ = textarea.focus();
        let _ = textarea.set_selection_range(end, end);
    });
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        );
    }
    closure.forget();
}

fn resize_prompt_textarea(input_id: &str) {
    let Some(textarea) = prompt_textarea(input_id) else {
        return;
    };
    let _ = textarea.set_attribute("style", "height:auto;overflow-y:hidden");
    let height = textarea.scroll_height().clamp(48, 192);
    let overflow = if height == 192 { "auto" } else { "hidden" };
    let _ = textarea.set_attribute("style", &format!("height:{height}px;overflow-y:{overflow}"));
}
