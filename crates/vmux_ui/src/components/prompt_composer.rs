use dioxus::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::prompt_ghost::PromptGhost;

pub use vmux_chat_ui::{PROMPT_INPUT_ID, PromptComposerAction, PromptComposerAttachment};

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
    let ghost = show_examples.then(|| {
        rsx! {
            PromptGhost {
                accent_bg,
                terminal: false,
            }
        }
    });
    rsx! {
        vmux_chat_ui::PromptComposer {
            value,
            preview,
            completion,
            attachments,
            ghost,
            footer,
            placeholder,
            accent_color,
            accent_gradient,
            action,
            action_title,
            action_enabled,
            on_input,
            on_keydown,
            on_paste,
            on_attach,
            on_remove_attachment,
            on_action,
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
