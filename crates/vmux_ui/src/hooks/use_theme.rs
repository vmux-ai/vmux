use super::use_listener;
use crate::host::Host;
use crate::i18n::Locale;
use crate::theme::{THEME_EVENT, ThemeEvent};
use dioxus::prelude::*;

/// Listens for [`ThemeEvent`] from the host and applies CSS custom properties.
pub fn use_theme() -> Signal<String> {
    let mut locale = use_signal(|| Locale::preferred().into_string());
    apply_locale(&Locale::from(locale().as_str()));
    let _listener = use_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        Host::set_root_radius(data.radius);
        let resolved = Locale::from(data.locale.as_str());
        if let Some(catalog) = data.catalog.as_deref() {
            let _ = resolved.register_catalog(catalog);
        }
        apply_locale(&resolved);
        locale.set(data.locale);
    });
    locale
}

fn apply_locale(locale: &Locale) {
    locale.make_current();
    let direction = match locale.direction() {
        unic_langid::CharacterDirection::RTL => "rtl",
        unic_langid::CharacterDirection::LTR => "ltr",
        unic_langid::CharacterDirection::TTB => "auto",
    };
    Host::set_root_language(locale.as_str(), direction);
}
