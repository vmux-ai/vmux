use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::overlay::WindowOverlay;
use vmux_ecs::page::PageReady;
use vmux_ecs::{UiStatePlugin, UiStateWrite};
use vmux_layout::LayoutCef;

use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;
use vmux_ui::theme::ThemeUiState;

pub(crate) struct BrowserLocale(String);

impl BrowserLocale {
    pub(crate) fn requested(locale: &str) -> Self {
        Self(Locale::requested(Some(locale)).into_string())
    }

    pub(crate) fn value(&self) -> &str {
        &self.0
    }

    pub(crate) fn accept_language_list(&self) -> String {
        let locale = self.0.trim();
        let language = locale.split('-').next().unwrap_or(locale);
        if language.eq_ignore_ascii_case("en") {
            if locale.eq_ignore_ascii_case(language) {
                return "en,en-US;q=0.9".to_string();
            }
            return format!("{locale},en;q=0.9");
        }
        if locale.eq_ignore_ascii_case(language) {
            return format!("{locale},en-US;q=0.9,en;q=0.8");
        }
        format!("{locale},{language};q=0.9,en-US;q=0.8,en;q=0.7")
    }

    fn catalog(&self) -> Option<String> {
        let directory = vmux_ecs::profile::ProfilePaths::current()
            .config()
            .join("locales");
        [self.0.as_str(), self.0.split('-').next().unwrap_or(&self.0)]
            .into_iter()
            .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
    }
}

struct BrowserAppearance<'a>(&'a AppSettings);

impl BrowserAppearance<'_> {
    fn color_mode(&self) -> CefColorMode {
        match self.0.appearance.mode {
            vmux_setting::ColorScheme::Light => CefColorMode::Light,
            vmux_setting::ColorScheme::Dark => CefColorMode::Dark,
            vmux_setting::ColorScheme::Device => CefColorMode::System,
        }
    }

    fn theme(&self) -> ThemeUiState {
        let locale = BrowserLocale::requested(&self.0.appearance.locale);
        ThemeUiState {
            radius: self.0.layout.radius,
            catalog: locale.catalog(),
            locale: locale.0,
        }
    }
}

pub(crate) struct AppearancePlugin;

impl Plugin for AppearancePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<ThemeUiState>::default())
            .add_observer(webview_ready_send_theme)
            .add_systems(
                Update,
                sync_to_cef
                    .before(CefSystems::CreateAndResize)
                    .run_if(resource_changed::<AppSettings>),
            )
            .add_systems(Update, reassert_color_scheme);
    }
}

fn reassert_color_scheme(
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

    browsers.set_color_scheme(BrowserAppearance(&settings).color_mode());
}

fn webview_ready_send_theme(
    trigger: On<UiInput<PageReady>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    cef_q: Query<(), With<LayoutCef>>,
    modal_q: Query<(), With<WindowOverlay>>,
    mut zoom_q: Query<&mut bevy_cef::prelude::ZoomLevel>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    commands.trigger(UiStateWrite::<ThemeUiState>::from_event(
        entity,
        &BrowserAppearance(&settings).theme(),
    ));
    if cef_q.get(entity).is_ok() || modal_q.get(entity).is_ok() {
        if let Ok(mut zoom) = zoom_q.get_mut(entity) {
            zoom.0 = 0.0;
        }
        browsers.set_zoom_level(&entity, 0.0);
    }
}

fn sync_to_cef(
    settings: Res<AppSettings>,
    mut scheme: ResMut<bevy_cef::prelude::CefColorScheme>,
    mut accept_language_list: Option<ResMut<bevy_cef::prelude::CefAcceptLanguageList>>,
    mut browsers: Option<NonSendMut<Browsers>>,
    ready: Query<Entity, With<PageReady>>,
    mut commands: Commands,
) {
    let appearance = BrowserAppearance(&settings);
    let mode = appearance.color_mode();
    if scheme.0 != mode {
        scheme.0 = mode;
    }
    let locale = BrowserLocale::requested(&settings.appearance.locale);
    let next_accept_language_list = locale.accept_language_list();
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
    let payload = appearance.theme();
    for entity in &ready {
        commands.trigger(UiStateWrite::<ThemeUiState>::from_event(entity, &payload));
    }
}

#[cfg(test)]
mod appearance_bridge_tests {
    use super::{BrowserAppearance, BrowserLocale};
    use bevy_cef::prelude::CefColorMode;
    use vmux_setting::{AppSettings, ColorScheme};

    #[test]
    fn maps_color_scheme_to_cef_mode() {
        let mut settings = AppSettings::default();
        settings.appearance.mode = ColorScheme::Light;
        assert_eq!(
            BrowserAppearance(&settings).color_mode(),
            CefColorMode::Light
        );
        settings.appearance.mode = ColorScheme::Dark;
        assert_eq!(
            BrowserAppearance(&settings).color_mode(),
            CefColorMode::Dark
        );
        settings.appearance.mode = ColorScheme::Device;
        assert_eq!(
            BrowserAppearance(&settings).color_mode(),
            CefColorMode::System
        );
    }

    #[test]
    fn selected_locale_leads_browser_accept_language() {
        assert_eq!(
            BrowserLocale("ja".to_string()).accept_language_list(),
            "ja,en-US;q=0.9,en;q=0.8"
        );
        assert_eq!(
            BrowserLocale("pt-BR".to_string()).accept_language_list(),
            "pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7"
        );
        assert_eq!(
            BrowserLocale("en-US".to_string()).accept_language_list(),
            "en-US,en;q=0.9"
        );
    }
}
