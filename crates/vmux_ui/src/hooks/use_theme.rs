use super::use_bin_event_listener;
use crate::i18n::{preferred_locale, register_catalog, set_current_locale, text_direction};
use crate::theme::{THEME_EVENT, ThemeEvent};
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

/// Listens for [`ThemeEvent`] from the Bevy host and applies CSS custom properties.
pub fn use_theme() -> Signal<String> {
    let mut locale = use_signal(preferred_locale);
    apply_locale(&locale());
    let _listener = use_bin_event_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        let css = format!("{}px", data.radius);
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        {
            let html: &web_sys::HtmlElement = el.unchecked_ref();
            let _ = html.style().set_property("--radius", &css);
        }
        if let Some(catalog) = data.catalog.as_deref() {
            let _ = register_catalog(&data.locale, catalog);
        }
        apply_locale(&data.locale);
        locale.set(data.locale);
    });
    locale
}

fn apply_locale(locale: &str) {
    set_current_locale(locale);
    let Some(el) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    let _ = el.set_attribute("lang", locale);
    let direction = match text_direction(locale) {
        unic_langid::CharacterDirection::RTL => "rtl",
        unic_langid::CharacterDirection::LTR => "ltr",
        unic_langid::CharacterDirection::TTB => "auto",
    };
    let _ = el.set_attribute("dir", direction);
}
