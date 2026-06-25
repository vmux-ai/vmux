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
    mut commands: Commands,
    new_pages: Query<
        (Entity, &ResolvedWebviewUri),
        (Added<ResolvedWebviewUri>, With<Browser>, Without<LayoutCef>),
    >,
) {
    let enabled = settings.map(|s| s.browser.vimium_enabled).unwrap_or(true);
    if !enabled {
        return;
    }
    for (entity, uri) in new_pages.iter() {
        if !vmux_vimium::is_web_scheme(&uri.0) {
            continue;
        }
        commands.entity(entity).insert(PreloadScripts(vec![
            vmux_vimium::preload_script().to_string(),
        ]));
    }
}
