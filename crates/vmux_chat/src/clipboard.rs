//! Clipboard access for the copy-message affordance.
//!
//! Only the CEF host can reach the navigator clipboard; elsewhere this is a no-op so the
//! transcript stays host-agnostic.

#[cfg(target_arch = "wasm32")]
pub fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn copy_to_clipboard(_text: &str) {}
