//! Clipboard access for the copy-message affordance.
//!
//! Only the CEF host can reach the navigator clipboard; elsewhere this is a no-op so the
//! transcript stays host-agnostic.

#[cfg(web)]
pub fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}

#[cfg(not(web))]
pub fn copy_to_clipboard(_text: &str) {}
