//! Keeping the pages looking like the app.
//!
//! CEF has its own notion of a colour scheme, so the setting is pushed into it whenever it
//! changes; a webview that becomes ready after that has missed the push and is told directly.

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::overlay::WindowOverlay;
use vmux_core::page::PageReady;
use vmux_layout::LayoutCef;

use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;
use vmux_ui::theme::THEME_EVENT;

use crate::{browser_accept_language_list, theme_event};
pub(crate) struct AppearancePlugin;

impl Plugin for AppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_webview_ready_send_theme)
            .add_systems(
                Update,
                sync_appearance_to_cef
                    .before(CefSystems::CreateAndResize)
                    .run_if(resource_changed::<AppSettings>),
            )
            .add_systems(Update, reassert_color_scheme_on_navigation);
    }
}

/// Tell CEF the colour scheme again whenever a page commits a navigation.
///
/// A pane's `prefers-color-scheme` is not a setting CEF holds but a media feature emulated over the
/// DevTools protocol, applied to a browser once when it is created. A navigation that crosses into
/// another renderer leaves the emulation behind with the old one, and the page that arrives asks the
/// platform instead — which on a dark desktop answers dark, whatever the app is set to.
///
/// Re-asserting costs one DevTools call per live browser on a navigation nobody sees the cost of,
/// and it also covers the startup ordering: the setting is pushed once, on the frame it changes, and
/// a browser created before that push would otherwise keep the default for its whole life.
fn reassert_color_scheme_on_navigation(
    mut committed: MessageReader<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>,
    settings: Res<AppSettings>,
    mut browsers: Option<NonSendMut<Browsers>>,
) {
    if committed.read().count() == 0 {
        return;
    }
    let Some(browsers) = browsers.as_deref_mut() else {
        return;
    };

    browsers.set_color_scheme(map_color_scheme(settings.appearance.mode));
}

fn on_webview_ready_send_theme(
    trigger: On<BinReceive<PageReady>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    cef_q: Query<(), With<LayoutCef>>,
    modal_q: Query<(), With<WindowOverlay>>,
    mut zoom_q: Query<&mut bevy_cef::prelude::ZoomLevel>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if browsers.can_emit_to(&entity) {
        let payload = theme_event(&settings);
        commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
    }
    // CEF / modal must never carry a stale zoom (e.g. from a previous
    // session where pinch-zoom was allowed); force them to 0 once the
    // webview is ready, both on the component and on the CEF host.
    if cef_q.get(entity).is_ok() || modal_q.get(entity).is_ok() {
        if let Ok(mut zoom) = zoom_q.get_mut(entity) {
            zoom.0 = 0.0;
        }
        browsers.set_zoom_level(&entity, 0.0);
    }
}

fn map_color_scheme(mode: vmux_setting::ColorScheme) -> bevy_cef::prelude::CefColorMode {
    match mode {
        vmux_setting::ColorScheme::Light => bevy_cef::prelude::CefColorMode::Light,
        vmux_setting::ColorScheme::Dark => bevy_cef::prelude::CefColorMode::Dark,
        vmux_setting::ColorScheme::Device => bevy_cef::prelude::CefColorMode::System,
    }
}

pub(crate) fn sync_appearance_to_cef(
    settings: Res<AppSettings>,
    mut scheme: ResMut<bevy_cef::prelude::CefColorScheme>,
    mut accept_language_list: Option<ResMut<bevy_cef::prelude::CefAcceptLanguageList>>,
    mut browsers: Option<NonSendMut<Browsers>>,
    ready: Query<Entity, With<PageReady>>,
    mut commands: Commands,
) {
    let mode = map_color_scheme(settings.appearance.mode);
    if scheme.0 != mode {
        scheme.0 = mode;
    }
    let locale = Locale::requested(Some(&settings.appearance.locale));
    let next_accept_language_list = browser_accept_language_list(locale.as_str());
    if accept_language_list
        .as_deref()
        .is_none_or(|current| current.0 != next_accept_language_list)
    {
        if let Some(current) = accept_language_list.as_deref_mut() {
            current.0 = next_accept_language_list.clone();
        }
        if let Some(browsers) = browsers.as_deref_mut() {
            browsers.set_accept_language_list(&next_accept_language_list);
        }
    }
    let browsers = browsers.as_deref();
    let Some(browsers) = browsers else { return };
    let payload = theme_event(&settings);
    for entity in &ready {
        if browsers.can_emit_to(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(entity, THEME_EVENT, &payload));
        }
    }
}

#[cfg(test)]
mod appearance_bridge_tests {
    use super::map_color_scheme;
    use bevy_cef::prelude::CefColorMode;
    use vmux_setting::ColorScheme;

    #[test]
    fn maps_color_scheme_to_cef_mode() {
        assert_eq!(map_color_scheme(ColorScheme::Light), CefColorMode::Light);
        assert_eq!(map_color_scheme(ColorScheme::Dark), CefColorMode::Dark);
        assert_eq!(map_color_scheme(ColorScheme::Device), CefColorMode::System);
    }
}
