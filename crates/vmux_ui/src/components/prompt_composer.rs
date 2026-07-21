use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::listener_guard::GuardedListener;
use crate::prompt_ghost::PromptGhost;

use super::prompt_box::PromptBox;

pub const PROMPT_INPUT_ID: &str = "vmux-prompt-input";

const PROMPT_COMPOSER_CSS: &str = r#"
.vmux-prompt-input{caret-color:transparent}
.vmux-prompt-caret{animation:vmux-prompt-caret-blink 1s step-end infinite;background:var(--vmux-prompt-accent)}
@keyframes vmux-prompt-caret-blink{0%,49%{opacity:1}50%,100%{opacity:0}}
.vmux-prompt-composer{border-color:rgba(255,255,255,0.18);box-shadow:0 22px 70px -28px rgba(255,255,255,0.2),inset 0 1px 0 rgba(255,255,255,0.16),inset 0 -1px 0 rgba(255,255,255,0.04)}
.vmux-prompt-composer:focus-within{border-color:rgba(255,255,255,0.28);box-shadow:0 26px 78px -26px rgba(255,255,255,0.28),inset 0 1px 0 rgba(255,255,255,0.2)}
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

struct PromptFocusTracking {
    window: web_sys::Window,
    focus: Closure<dyn FnMut()>,
    blur: Closure<dyn FnMut()>,
}

impl Drop for PromptFocusTracking {
    fn drop(&mut self) {
        let _ = self
            .window
            .remove_event_listener_with_callback("focus", self.focus.as_ref().unchecked_ref());
        let _ = self
            .window
            .remove_event_listener_with_callback("blur", self.blur.as_ref().unchecked_ref());
    }
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
    let mut caret = use_signal(|| None::<u32>);
    let scroll_top = use_signal(|| 0i32);

    let focus_listener = use_hook(|| {
        GuardedListener::new(Box::new(move |_: ()| {
            sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top);
        }) as Box<dyn FnMut(())>)
    });
    let blur_listener = use_hook(|| {
        GuardedListener::new(Box::new(move |_: ()| {
            caret.set(None);
        }) as Box<dyn FnMut(())>)
    });
    let focus_tracking = use_hook(|| Rc::new(RefCell::new(None::<PromptFocusTracking>)));
    use_effect({
        let focus_listener = focus_listener.clone();
        let blur_listener = blur_listener.clone();
        let focus_tracking = focus_tracking.clone();
        move || {
            *focus_tracking.borrow_mut() =
                install_prompt_focus_tracking(focus_listener.clone(), blur_listener.clone());
        }
    });
    let focus_guard = focus_listener.guard();
    let blur_guard = blur_listener.guard();
    use_drop(move || {
        focus_guard.deactivate();
        blur_guard.deactivate();
        focus_tracking.borrow_mut().take();
    });

    use_effect(use_reactive((&value,), move |_| {
        resize_prompt_textarea(PROMPT_INPUT_ID);
        sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top);
    }));

    let caret_prefix = caret()
        .filter(|_| !value.is_empty())
        .map(|offset| prompt_prefix_at_utf16(&value, offset).to_string());
    let prompt_scroll_offset = scroll_top();
    let action_class = if action_enabled {
        match action {
            PromptComposerAction::Send => format!(
                "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg bg-gradient-to-br text-white shadow-lg transition hover:brightness-110 active:scale-95 {accent_gradient}"
            ),
            PromptComposerAction::Stop => "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg bg-white/40 text-foreground/70 shadow-sm ring-1 ring-inset ring-black/10 transition hover:bg-white/60 hover:text-foreground active:scale-95 dark:bg-white/[0.08] dark:ring-white/10 dark:hover:bg-white/[0.14]".to_string(),
        }
    } else {
        "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 cursor-default self-center items-center justify-center rounded-lg bg-white/25 text-muted-foreground/35 shadow-sm ring-1 ring-inset ring-black/[0.06] dark:bg-white/[0.055] dark:ring-white/[0.08]".to_string()
    };

    rsx! {
        style { dangerous_inner_html: PROMPT_COMPOSER_CSS }
        PromptBox {
            vertical: true,
            class: "vmux-prompt-composer",
            style: "--vmux-prompt-accent:{accent_color};",
            div { class: "relative z-10 flex w-full items-center",
                button {
                class: "relative z-10 ml-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg text-foreground/45 transition hover:bg-foreground/10 hover:text-foreground",
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
                div { class: "relative z-10 flex min-w-0 flex-1 flex-wrap items-center gap-1 px-2",
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
                                class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-foreground/45 transition hover:bg-foreground/10 hover:text-foreground",
                                title: "Remove attachment",
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
                div { class: "relative min-w-32 flex-1 overflow-hidden",
                    if value.is_empty() {
                        div { class: "pointer-events-none absolute inset-0 flex -translate-y-px items-center overflow-hidden px-1.5",
                            if !preview.is_empty() {
                                div { class: "max-w-full truncate whitespace-nowrap text-[15px] leading-6 text-foreground", "{preview}" }
                            } else if show_examples {
                                PromptGhost {
                                    accent_bg,
                                    terminal: false,
                                }
                            } else {
                                div { class: "flex max-w-full items-center whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50",
                                    if caret().is_some() {
                                        span { class: "vmux-prompt-caret relative top-px mr-px h-4 w-1.5 shrink-0" }
                                    }
                                    span { class: "min-w-0 truncate", "{placeholder}" }
                                }
                            }
                        }
                    }
                    if !completion.is_empty() {
                        div {
                            class: "pointer-events-none absolute inset-0 overflow-hidden whitespace-pre-wrap break-words px-1.5 py-2 text-[15px] leading-6",
                            span { class: "text-transparent", "{value}" }
                            span { class: "text-muted-foreground/40", "{completion}" }
                        }
                    }
                    if let Some(prefix) = caret_prefix.as_ref() {
                        div { class: "pointer-events-none absolute inset-0 z-20 overflow-hidden",
                            div {
                                class: "min-h-10 w-full whitespace-pre-wrap break-words px-1.5 py-2 text-[15px] leading-6 text-transparent",
                                style: "transform:translateY(-{prompt_scroll_offset}px);",
                                span { "{prefix}" }
                                span { class: "vmux-prompt-caret relative top-px ml-px inline-block h-4 w-1.5 align-middle" }
                            }
                        }
                    }
                    textarea {
                        id: PROMPT_INPUT_ID,
                        class: "vmux-prompt-input relative z-10 max-h-40 min-h-10 w-full resize-none bg-transparent px-1.5 py-2 text-[15px] leading-6 placeholder:text-transparent focus:outline-none",
                        autofocus: true,
                        rows: "1",
                        placeholder: "{placeholder}",
                        value: "{value}",
                        oninput: move |event| {
                            on_input.call(event.value());
                            resize_prompt_textarea(PROMPT_INPUT_ID);
                            sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top);
                        },
                        onpaste: move |_| on_paste.call(()),
                        onfocus: move |_| sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top),
                        onblur: move |_| sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top),
                        onkeyup: move |_| sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top),
                        onmouseup: move |_| sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top),
                        onscroll: move |_| sync_prompt_caret(PROMPT_INPUT_ID, caret, scroll_top),
                        onkeydown: move |event| on_keydown.call(event),
                    }
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
            if let Some(footer) = footer {
                div { class: "relative z-10 w-full", {footer} }
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
    let height = textarea.scroll_height().clamp(40, 160);
    let overflow = if height == 160 { "auto" } else { "hidden" };
    let _ = textarea.set_attribute("style", &format!("height:{height}px;overflow-y:{overflow}"));
}

fn sync_prompt_caret(input_id: &str, mut caret: Signal<Option<u32>>, mut scroll_top: Signal<i32>) {
    let page_active = web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.has_focus().unwrap_or(false));
    if !page_active {
        caret.set(None);
        return;
    }
    let Some(textarea) = prompt_textarea(input_id) else {
        return;
    };
    let start = textarea.selection_start().ok().flatten().unwrap_or(0);
    let end = textarea.selection_end().ok().flatten().unwrap_or(start);
    caret.set((start == end).then_some(start));
    scroll_top.set(textarea.scroll_top());
}

fn install_prompt_focus_tracking(
    focus_listener: GuardedListener<Box<dyn FnMut(())>>,
    blur_listener: GuardedListener<Box<dyn FnMut(())>>,
) -> Option<PromptFocusTracking> {
    let window = web_sys::window()?;
    let focus = Closure::wrap(Box::new(move || {
        focus_listener.call(());
    }) as Box<dyn FnMut()>);
    let _ = window.add_event_listener_with_callback("focus", focus.as_ref().unchecked_ref());

    let blur = Closure::wrap(Box::new(move || {
        blur_listener.call(());
    }) as Box<dyn FnMut()>);
    let _ = window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref());

    Some(PromptFocusTracking {
        window,
        focus,
        blur,
    })
}

fn prompt_prefix_at_utf16(value: &str, offset: u32) -> &str {
    let mut units = 0u32;
    for (byte, character) in value.char_indices() {
        if units >= offset {
            return &value[..byte];
        }
        units += character.len_utf16() as u32;
    }
    value
}
