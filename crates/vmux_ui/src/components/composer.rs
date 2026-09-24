use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_api::prompt_media::ChatAttachment;

use crate::components::prompt_box::PromptBox;
use crate::file_icon::FilePath;
use crate::i18n::translate;
use crate::ime::use_ime_guard;
use crate::util::cn;

pub const PROMPT_INPUT_ID: &str = "vmux-prompt-input";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptComposerAttachment {
    pub key: String,
    pub name: String,
    pub label: String,
    pub preview_data_url: String,
    pub remove_index: Option<usize>,
}

impl PromptComposerAttachment {
    pub fn from_attachment(
        attachment: &ChatAttachment,
        previews: &HashMap<String, ChatAttachment>,
        remove_index: Option<usize>,
    ) -> Self {
        let loaded = previews
            .get(&attachment.path)
            .map(|preview| preview.preview_data_url.as_str())
            .filter(|url| !url.is_empty())
            .unwrap_or(attachment.preview_data_url.as_str());
        let held = match remove_index {
            Some(_) => "attachment",
            None => "pinned-attachment",
        };
        Self {
            key: format!("{held}-{}", attachment.path),
            name: attachment.name.clone(),
            label: FilePath(&attachment.name).extension_label(),
            preview_data_url: loaded.to_string(),
            remove_index,
        }
    }

    pub fn removable(
        attachments: &[ChatAttachment],
        previews: &HashMap<String, ChatAttachment>,
    ) -> Vec<Self> {
        let mut listed = Vec::with_capacity(attachments.len());
        for (index, attachment) in attachments.iter().enumerate() {
            listed.push(Self::from_attachment(attachment, previews, Some(index)));
        }
        listed
    }

    pub fn pinned(
        attachments: &[ChatAttachment],
        previews: &HashMap<String, ChatAttachment>,
    ) -> Vec<Self> {
        let mut listed = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            listed.push(Self::from_attachment(attachment, previews, None));
        }
        listed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptComposerMode {
    #[default]
    Send,
    Stop,
}

#[component]
pub fn PromptComposer(
    value: String,
    #[props(default)] preview: String,
    #[props(default)] overlay: String,
    #[props(default)] completion: String,
    #[props(default)] attachments: Vec<PromptComposerAttachment>,
    #[props(default)] ghost: Option<Element>,
    placeholder: String,
    accent_color: String,
    accent_gradient: String,
    #[props(default)] footer: Option<Element>,
    #[props(default)] shared_transition: bool,
    #[props(default = true)] show_send_button: bool,
    #[props(default = PROMPT_INPUT_ID.to_string())] input_id: String,
    #[props(default = translate("composer-attach-files"))] attach_title: String,
    #[props(default = translate("composer-remove-attachment"))] remove_attachment_title: String,
    #[props(default)] mode: PromptComposerMode,
    action_title: String,
    action_enabled: bool,
    on_input: EventHandler<String>,
    on_keydown: EventHandler<KeyboardEvent>,
    on_paste: EventHandler<()>,
    on_attach: EventHandler<()>,
    on_remove_attachment: EventHandler<usize>,
    on_action: EventHandler<()>,
) -> Element {
    let footer = footer.or_else(|| {
        Some(rsx! {
            div { class: "truncate text-[10px] text-muted-foreground/55", {translate("command-send")} }
        })
    });
    let ime = use_ime_guard();
    let has_ghost = ghost.is_some();
    let overlaid = !overlay.is_empty();
    let typed_text_class = if overlaid { "text-transparent" } else { "" };
    let mode_class = if action_enabled {
        match mode {
            PromptComposerMode::Send => cn([
                "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl bg-gradient-to-br text-white shadow-lg transition active:scale-95 hover:brightness-110 sm:h-8 sm:w-8 sm:rounded-lg",
                accent_gradient.as_str(),
            ]),
            PromptComposerMode::Stop => "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl bg-white/10 text-foreground/70 shadow-sm ring-1 ring-inset ring-white/10 transition active:scale-95 hover:bg-white/60 hover:text-foreground sm:h-8 sm:w-8 sm:rounded-lg sm:bg-white/40 sm:ring-black/10 dark:sm:bg-white/[0.08] dark:sm:ring-white/10 dark:hover:bg-white/[0.14]".to_string(),
        }
    } else {
        "relative z-10 mr-0.5 flex h-11 w-11 shrink-0 cursor-default self-center items-center justify-center rounded-xl bg-white/[0.055] text-muted-foreground/35 shadow-sm ring-1 ring-inset ring-white/[0.08] sm:h-8 sm:w-8 sm:rounded-lg sm:bg-white/25 sm:ring-black/[0.06] dark:sm:bg-white/[0.055] dark:sm:ring-white/[0.08]".to_string()
    };
    let shared_transition_class = if shared_transition {
        "vmux-agent-composer-shared"
    } else {
        ""
    };
    let prompt_box_class = cn(["vmux-prompt-composer flex-wrap", shared_transition_class]);
    let textarea_class = cn([
        "relative z-10 max-h-40 min-h-11 w-full [field-sizing:content] resize-none overflow-y-auto bg-transparent px-1.5 py-2.5 text-base leading-6 caret-[var(--vmux-prompt-accent)] outline-none placeholder:overflow-hidden placeholder:text-ellipsis placeholder:whitespace-nowrap placeholder:text-muted-foreground/50 sm:min-h-10 sm:py-2 sm:text-[15px]",
        typed_text_class,
    ]);

    rsx! {
        PromptBox {
            class: prompt_box_class,
            style: "--vmux-prompt-accent:{accent_color};",
            button {
                class: "relative z-10 ml-0.5 flex h-11 w-11 shrink-0 self-center items-center justify-center rounded-xl text-foreground/45 transition active:bg-foreground/10 active:text-foreground hover:bg-foreground/10 hover:text-foreground sm:h-8 sm:w-8 sm:rounded-lg",
                r#type: "button",
                title: "{attach_title}",
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

            if !attachments.is_empty() {
                div { class: "relative z-10 order-first flex w-full flex-wrap items-start gap-1.5 px-1.5 pt-1.5",
                    for attachment in attachments.iter().cloned() {
                        div {
                            key: "{attachment.key}",
                            class: "group relative shrink-0",
                            title: "{attachment.name}",
                            if attachment.preview_data_url.is_empty() {
                                div { class: "flex h-7 max-w-56 items-center gap-1.5 rounded-full bg-foreground/[0.08] pl-1 pr-2 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/10",
                                    span { class: "flex h-5 min-w-5 items-center justify-center rounded-full bg-foreground/[0.08] px-1 font-mono text-[8px] font-semibold text-muted-foreground",
                                        "{attachment.label}"
                                    }
                                    span { class: "min-w-0 max-w-40 truncate", "{attachment.name}" }
                                }
                            } else {
                                img {
                                    src: "{attachment.preview_data_url}",
                                    alt: "{attachment.name}",
                                    class: "h-24 w-auto max-w-72 rounded-xl object-cover",
                                }
                            }
                            if let Some(remove_index) = attachment.remove_index {
                                button {
                                    class: "absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full bg-background/90 text-foreground/60 opacity-0 shadow-sm ring-1 ring-inset ring-foreground/15 transition group-hover:opacity-100 active:bg-foreground/10 active:text-foreground hover:text-foreground",
                                    r#type: "button",
                                    title: "{remove_attachment_title}",
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
            div { class: "relative z-10 flex min-w-0 flex-1 flex-wrap items-center gap-1 px-1 sm:px-2",
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
                    if overlaid {
                        div { class: "pointer-events-none absolute inset-0 flex items-center overflow-hidden px-1.5",
                            div { class: "max-w-full truncate whitespace-nowrap text-base leading-6 text-foreground sm:text-[15px]", "{overlay}" }
                        }
                    } else if !completion.is_empty() {
                        div {
                            class: "pointer-events-none absolute inset-0 overflow-hidden whitespace-pre-wrap break-words px-1.5 py-2.5 text-base leading-6 sm:py-2 sm:text-[15px]",
                            span { class: "text-transparent", "{value}" }
                            span { class: "text-muted-foreground/40", "{completion}" }
                        }
                    }
                    textarea {
                        id: "{input_id}",
                        class: textarea_class,
                        autofocus: true,
                        rows: "1",
                        spellcheck: "false",
                        autocapitalize: "off",
                        autocomplete: "off",
                        "autocorrect": "off",
                        placeholder: if preview.is_empty() && !has_ghost && !overlaid { placeholder } else { String::new() },
                        value: "{value}",
                        oninput: move |event| {
                            if let Some(value) = ime.input(event.value()) {
                                on_input.call(value);
                            }
                        },
                        onpaste: move |_| on_paste.call(()),
                        oncompositionstart: move |_| ime.start(),
                        oncompositionend: move |_| {
                            if let Some(value) = ime.commit_input() {
                                on_input.call(value);
                            }
                        },
                        onkeydown: move |event| {
                            if ime.swallows(&event) {
                                return;
                            }
                            on_keydown.call(event);
                        },
                    }
                }
            }
            if mode == PromptComposerMode::Stop || show_send_button {
                button {
                    class: "{mode_class}",
                    r#type: "button",
                    disabled: !action_enabled,
                    title: "{action_title}",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        if action_enabled {
                            on_action.call(());
                        }
                    },
                    if mode == PromptComposerMode::Stop {
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
            if let Some(footer) = footer {
                div { class: "relative z-10 order-last w-full px-2 pb-1", {footer} }
            }
        }
    }
}

pub fn focus_prompt_end(input_id: impl Into<std::borrow::Cow<'static, str>>) {
    crate::focus::FocusClaim::new(input_id)
        .caret_at_end()
        .request();
}
