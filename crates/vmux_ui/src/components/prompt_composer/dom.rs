//! Caret and focus helpers for the shared composer textarea.
//!
//! Only the CEF host can reach the DOM directly; on every other target these are no-ops so the
//! composer itself stays host-agnostic.

#[cfg(web)]
pub fn prompt_textarea(input_id: &str) -> Option<web_sys::HtmlTextAreaElement> {
    use wasm_bindgen::JsCast;

    web_sys::window()?
        .document()?
        .get_element_by_id(input_id)?
        .dyn_into()
        .ok()
}

/// Move the caret to the end of the composer input on the next tick.
#[cfg(web)]
pub fn focus_prompt_end(input_id: &str) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

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

#[cfg(not(web))]
pub fn focus_prompt_end(_input_id: &str) {}
