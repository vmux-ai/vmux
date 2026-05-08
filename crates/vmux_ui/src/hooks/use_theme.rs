use super::use_bin_event_listener;
use crate::theme::{THEME_EVENT, ThemeEvent};
use wasm_bindgen::JsCast;

/// Listens for [`ThemeEvent`] from the Bevy host and applies CSS custom properties.
pub fn use_theme() {
    let _listener = use_bin_event_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        let css = format!("{}px", data.radius);
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        {
            let html: &web_sys::HtmlElement = el.unchecked_ref();
            let _ = html.style().set_property("--radius", &css);
        }
    });
}
