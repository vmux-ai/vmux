use crate::theme::{ThemeEvent, THEME_EVENT};
use super::use_event_listener;

/// Listens for [`ThemeEvent`] from the Bevy host and applies CSS custom properties.
pub fn use_theme() {
    let _listener = use_event_listener::<ThemeEvent, _>(THEME_EVENT, move |data| {
        let css = format!("{}px", data.radius);
        dioxus::prelude::document::eval(&format!(
            "document.documentElement.style.setProperty('--radius', '{css}')"
        ));
    });
}
