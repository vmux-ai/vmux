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

/// A locale tag safe to inline into a script literal: BCP-47 is alphanumerics and `-`, so a tag
/// that matches cannot terminate a string or introduce syntax. `ThemeEvent` arrives over IPC, so
/// this is validated rather than escaped — anything unexpected is dropped, not sanitised.
#[cfg(not(target_arch = "wasm32"))]
fn is_plain_locale_tag(locale: &str) -> bool {
    !locale.is_empty()
        && locale.len() <= 35
        && locale
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(not(target_arch = "wasm32"))]
fn set_root_radius(radius: f32) {
    if !radius.is_finite() {
        return;
    }
    // `document::eval` is Dioxus's WebView bridge, not JS `eval`. The only interpolated value is
    // a finite f32, whose `Display` output is digits, `.`, `-`, and `e`.
    document::eval(&format!(
        "document.documentElement.style.setProperty('--radius', '{radius}px');"
    ));
}

#[cfg(not(target_arch = "wasm32"))]
fn set_root_language(locale: &str, direction: &str) {
    // `direction` is one of three compile-time literals from `apply_locale`; `locale` is checked
    // against the BCP-47 character set above.
    if !is_plain_locale_tag(locale) {
        return;
    }
    document::eval(&format!(
        "document.documentElement.setAttribute('lang', '{locale}');\
         document.documentElement.setAttribute('dir', '{direction}');"
    ));
}
