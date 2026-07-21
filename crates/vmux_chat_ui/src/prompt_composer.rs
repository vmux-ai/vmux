use dioxus::prelude::*;

use crate::{PromptBox, PromptBoxTone};

/// Default DOM id for the shared prompt textarea.
pub const PROMPT_INPUT_ID: &str = "vmux-prompt-input";

/// One attachment pill rendered inside a prompt composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptComposerAttachment {
    pub key: String,
    pub name: String,
    pub label: String,
    pub preview_data_url: String,
    pub remove_index: Option<usize>,
}

/// Primary action displayed by a prompt composer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptComposerAction {
    #[default]
    Send,
    Stop,
}

#[component]
/// Shared prompt composer used by desktop and mobile chat surfaces.
pub fn PromptComposer(
    value: String,
    #[props(default)] preview: String,
    #[props(default)] completion: String,
    #[props(default)] attachments: Vec<PromptComposerAttachment>,
    #[props(default)] ghost: Option<Element>,
    placeholder: String,
    accent_color: String,
    accent_gradient: String,
    #[props(default)] tone: PromptBoxTone,
    #[props(default = true)] autofocus: bool,
    #[props(default = true)] show_attach: bool,
    #[props(default)] disabled: bool,
    #[props(default)] footer: Option<Element>,
    #[props(default = PROMPT_INPUT_ID.to_string())] input_id: String,
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
    let has_ghost = ghost.is_some();
    let action_class = if action_enabled {
        match action {
            PromptComposerAction::Send => format!(
                "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl bg-gradient-to-br text-white shadow-lg transition active:scale-95 sm:h-8 sm:w-8 sm:rounded-lg sm:hover:brightness-110 {accent_gradient}"
            ),
            PromptComposerAction::Stop => "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl bg-white/10 text-foreground/70 shadow-sm ring-1 ring-inset ring-white/10 transition active:scale-95 sm:h-8 sm:w-8 sm:rounded-lg sm:bg-white/40 sm:ring-black/10 sm:hover:bg-white/60 sm:hover:text-foreground dark:sm:bg-white/[0.08] dark:sm:ring-white/10 dark:sm:hover:bg-white/[0.14]".to_string(),
        }
    } else {
        "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 cursor-default self-center items-center justify-center rounded-xl bg-white/[0.055] text-muted-foreground/35 shadow-sm ring-1 ring-inset ring-white/[0.08] sm:h-8 sm:w-8 sm:rounded-lg sm:bg-white/25 sm:ring-black/[0.06] dark:sm:bg-white/[0.055] dark:sm:ring-white/[0.08]".to_string()
    };

    rsx! {
        PromptBox {
            tone,
            class: "vmux-prompt-composer flex-wrap",
            style: "--vmux-prompt-accent:{accent_color};",
            if show_attach {
                button {
                    class: "relative z-10 ml-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl text-foreground/45 transition active:bg-foreground/10 active:text-foreground sm:h-8 sm:w-8 sm:rounded-lg sm:hover:bg-foreground/10 sm:hover:text-foreground",
                    r#type: "button",
                    disabled,
                    title: "Attach files (/upload)",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| on_attach.call(()),
                    svg {
                        class: "h-4 w-4",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" }
                    }
                }
            }
            div { class: "relative z-10 flex min-w-0 flex-1 flex-wrap items-center gap-1 px-1 sm:px-2",
                for attachment in attachments.iter().cloned() {
                    div {
                        key: "{attachment.key}",
                        class: if attachment.remove_index.is_some() { "group flex h-7 max-w-56 shrink-0 items-center gap-1.5 rounded-full bg-foreground/[0.08] pl-1 pr-1.5 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/10" } else { "flex h-7 max-w-56 shrink-0 items-center gap-1.5 rounded-full bg-foreground/[0.08] pl-1 pr-2 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/10" },
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
                                class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-foreground/45 transition active:bg-foreground/10 active:text-foreground sm:hover:bg-foreground/10 sm:hover:text-foreground",
                                r#type: "button",
                                title: "Remove attachment",
                                onmousedown: move |event| event.prevent_default(),
                                onclick: move |_| on_remove_attachment.call(remove_index),
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
                div { class: "relative min-w-32 flex-1 overflow-hidden",
                    if value.is_empty() {
                        if !preview.is_empty() {
                            div { class: "pointer-events-none absolute inset-0 flex items-center overflow-hidden px-1.5",
                                div { class: "max-w-full truncate whitespace-nowrap text-base leading-6 text-foreground sm:text-[15px]", "{preview}" }
                            }
                        } else if let Some(ghost) = ghost {
                            div { class: "pointer-events-none absolute inset-0 flex items-center overflow-hidden px-1.5", {ghost} }
                        }
                    }
                    if !completion.is_empty() {
                        div {
                            class: "pointer-events-none absolute inset-0 overflow-hidden whitespace-pre-wrap break-words px-1.5 py-2.5 text-base leading-6 sm:py-2 sm:text-[15px]",
                            span { class: "text-transparent", "{value}" }
                            span { class: "text-muted-foreground/40", "{completion}" }
                        }
                    }
                    textarea {
                        id: "{input_id}",
                        class: "relative z-10 max-h-40 min-h-11 w-full [field-sizing:content] resize-none overflow-y-auto bg-transparent px-1.5 py-2.5 text-base leading-6 caret-[var(--vmux-prompt-accent)] outline-none placeholder:text-muted-foreground/50 sm:min-h-10 sm:py-2 sm:text-[15px]",
                        autofocus,
                        disabled,
                        rows: "1",
                        placeholder: if preview.is_empty() && !has_ghost { placeholder } else { String::new() },
                        value: "{value}",
                        oninput: move |event| on_input.call(event.value()),
                        onpaste: move |_| on_paste.call(()),
                        onkeydown: move |event| on_keydown.call(event),
                    }
                }
            }
            button {
                class: "{action_class}",
                r#type: "button",
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
            if let Some(footer) = footer {
                div { class: "relative z-10 order-last w-full px-2 pb-1", {footer} }
            }
        }
    }
}
