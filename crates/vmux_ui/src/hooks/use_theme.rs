use super::use_listener;
use crate::hooks::Host;
use crate::i18n::{preferred_locale, register_catalog, set_current_locale, text_direction};
use crate::theme::{THEME_EVENT, ThemeEvent};
use dioxus::prelude::*;

/// Listens for [`ThemeEvent`] from the host and applies CSS custom properties.
pub fn use_theme() -> Signal<String> {
    let mut locale = use_signal(preferred_locale);
    apply_locale(&locale());
    let _listener = use_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        Host::set_root_radius(data.radius);
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
    Host::set_root_language(locale, direction);
}
