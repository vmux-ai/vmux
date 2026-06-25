use bevy::prelude::*;
use bevy_cef::prelude::{CefSystems, PreloadScripts, ResolvedWebviewUri};
use vmux_layout::{Browser, LayoutCef};
use vmux_setting::AppSettings;

pub struct VimiumPlugin;

impl Plugin for VimiumPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            set_vimium_preload.before(CefSystems::CreateAndResize),
        );
    }
}

fn set_vimium_preload(
    settings: Option<Res<AppSettings>>,
    mut new_pages: Query<
        (&ResolvedWebviewUri, &mut PreloadScripts),
        (Added<ResolvedWebviewUri>, With<Browser>, Without<LayoutCef>),
    >,
) {
    let enabled = settings.map(|s| s.browser.vimium_enabled).unwrap_or(true);
    if !enabled {
        return;
    }
    let script = vmux_vimium::preload_script();
    for (uri, mut preload) in new_pages.iter_mut() {
        if !vmux_vimium::is_web_scheme(&uri.0) {
            continue;
        }
        if !preload.0.iter().any(|s| s == script) {
            preload.0.push(script.to_string());
        }
    }
}
