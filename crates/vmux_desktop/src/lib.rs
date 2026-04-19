mod browser;
mod command;
pub(crate) mod keybinding;
mod layout;
mod profile;
mod os_menu;
mod scene;
mod settings;
mod unit;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, PrimaryWindow, Window as NativeWindow, WindowPlugin};
use bevy::winit::WinitWindows;

use {
    browser::BrowserPlugin, command::CommandPlugin, keybinding::KeyBindingPlugin,
    layout::LayoutPlugin, os_menu::OsMenuPlugin, profile::ProfilePlugin,
    scene::ScenePlugin, settings::SettingsPlugin,
    vmux_header::HeaderPlugin, vmux_side_sheet::SideSheetWebviewPlugin,
    vmux_webview_app::WebviewAppRegistryPlugin,
};

pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let primary_window = NativeWindow {
            transparent: true,
            composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
            decorations: true,
            titlebar_shown: false,
            movable_by_window_background: false,
            fullsize_content_view: true,
            ..default()
        };
        let window_plugin = WindowPlugin {
            primary_window: Some(primary_window),
            ..default()
        };

        app.add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin)
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    ..default()
                }),
            SettingsPlugin,
            CommandPlugin,
            KeyBindingPlugin,
            ScenePlugin,
            OsMenuPlugin,
            WebviewAppRegistryPlugin,
            HeaderPlugin,
            SideSheetWebviewPlugin,
            BrowserPlugin,
            ProfilePlugin,
            LayoutPlugin,
        ))
        .register_type::<layout::space::Space>()
        .register_type::<layout::pane::Pane>()
        .register_type::<layout::pane::PaneSplit>()
        .register_type::<layout::pane::PaneSplitDirection>()
        .register_type::<layout::tab::Tab>()
        .register_type::<vmux_history::CreatedAt>()
        .register_type::<vmux_history::LastActivatedAt>()
        .register_type::<vmux_history::Visit>()
        .add_systems(Update, fit_window_to_screen.run_if(not(resource_exists::<ScreenFitted>)));
    }
}

#[derive(Resource)]
pub(crate) struct ScreenFitted;

fn fit_window_to_screen(
    winit_windows: Option<NonSend<WinitWindows>>,
    mut window_q: Query<(Entity, &mut NativeWindow), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Some(winit_windows) = winit_windows else {
        return;
    };
    let Ok((entity, mut window)) = window_q.single_mut() else {
        return;
    };
    let Some(winit_win) = winit_windows.get_window(entity) else {
        return;
    };
    let Some(monitor) = winit_win.current_monitor() else {
        return;
    };
    let size = monitor.size();
    let scale = monitor.scale_factor() as f32;
    let logical_w = size.width as f32 / scale;
    let logical_h = size.height as f32 / scale;
    window.resolution.set(logical_w, logical_h);
    commands.insert_resource(ScreenFitted);
}
