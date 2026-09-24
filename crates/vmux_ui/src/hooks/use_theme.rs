use super::use_ui_state;
use crate::i18n::Locale;
use crate::theme::ThemeEvent;
use crate::transport::Host;
use dioxus::prelude::*;

pub fn use_theme() -> Signal<String> {
    let state = use_ui_state::<ThemeEvent>();
    let mut locale = use_signal(|| Locale::preferred().into_string());
    apply_locale(&Locale::from(locale().as_str()));
    use_effect(move || {
        let data = state();
        if data.locale.is_empty() {
            return;
        }
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
