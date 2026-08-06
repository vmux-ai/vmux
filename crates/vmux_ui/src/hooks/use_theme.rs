use super::use_bin_event_listener;
use crate::i18n::{preferred_locale, register_catalog, set_current_locale, text_direction};
use crate::theme::{THEME_EVENT, ThemeEvent};
use dioxus::prelude::*;

/// Listens for [`ThemeEvent`] from the host and applies CSS custom properties.
pub fn use_theme() -> Signal<String> {
    let mut locale = use_signal(preferred_locale);
    apply_locale(&locale());
    let _listener = use_bin_event_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        set_root_radius(data.radius);
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
    let direction = match text_direction(locale) {
        unic_langid::CharacterDirection::RTL => "rtl",
        unic_langid::CharacterDirection::LTR => "ltr",
        unic_langid::CharacterDirection::TTB => "auto",
    };
    set_root_language(locale, direction);
}

/// Writing to the document element is the one part of theming that cannot be shared: wasm reaches
/// it through `web_sys`, a native WebView host through an eval bridge.
#[cfg(target_arch = "wasm32")]
fn set_root_radius(radius: f32) {
    use wasm_bindgen::JsCast;

    let Some(el) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    let html: &web_sys::HtmlElement = el.unchecked_ref();
    let _ = html
        .style()
        .set_property("--radius", &format!("{radius}px"));
}

#[cfg(target_arch = "wasm32")]
fn set_root_language(locale: &str, direction: &str) {
    let Some(el) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    let _ = el.set_attribute("lang", locale);
    let _ = el.set_attribute("dir", direction);
}

/// Off CEF there is nothing to write to.
///
/// `ThemeEvent` is only ever sent by the CEF host (`vmux_browser`), so the radius never changes
/// on another host and `theme.css` already carries its default. Locale still resolves — the
/// returned signal and [`text_direction`] are the contract — but a native host applies it on its
/// own root element rather than reaching for the document.
#[cfg(not(target_arch = "wasm32"))]
fn set_root_radius(_radius: f32) {}

#[cfg(not(target_arch = "wasm32"))]
fn set_root_language(_locale: &str, _direction: &str) {}
