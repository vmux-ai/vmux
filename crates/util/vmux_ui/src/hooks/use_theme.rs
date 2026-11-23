use super::use_ui_state;
use crate::i18n::Locale;
use crate::theme::ThemeUiState;
use crate::transport::Host;
use dioxus::prelude::*;

pub fn use_theme() -> Signal<String> {
    let state = use_ui_state::<ThemeUiState>().state;
    let mut locale = use_signal(|| Locale::preferred().into_string());
    Host::apply_locale(&Locale::from(locale().as_str()));
    use_effect(move || {
        let data = state();
        if data.locale.is_empty() {
            return;
        }
        Host::set_root_radius(data.radius);
        let scheme = data.scheme.as_str();
        Host::run_script(format!(
            "document.documentElement.dataset.theme='{scheme}';document.documentElement.style.colorScheme='{scheme}';"
        ));
        let resolved = Locale::from(data.locale.as_str());
        if let Some(catalog) = data.catalog.as_deref() {
            let _ = resolved.register_catalog(catalog);
        }
        Host::apply_locale(&resolved);
        locale.set(data.locale);
    });
    locale
}
