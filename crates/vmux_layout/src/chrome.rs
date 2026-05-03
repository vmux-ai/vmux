use std::path::PathBuf;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy_cef::prelude::*;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

use crate::event::{LAYOUT_WEBVIEW_URL, TERMINAL_WEBVIEW_URL};
use crate::window::WEBVIEW_MESH_DEPTH_BIAS;

#[derive(Component)]
pub struct Browser;

#[derive(Component)]
pub struct LayoutChrome;

#[derive(Component)]
pub struct Loading;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component)]
pub struct NavigationState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub struct LayoutChromePlugin;

impl Plugin for LayoutChromePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("layout"),
            );
    }
}

pub fn apply_chrome_state_from_cef(
    chrome_rx: Res<WebviewChromeStateReceiver>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata>,
) {
    while let Ok(ev) = chrome_rx.0.try_recv() {
        let Ok(mut meta) = browser_meta.get_mut(ev.webview) else {
            continue;
        };
        if let Some(url) = ev.url
            && !meta.url.starts_with(TERMINAL_WEBVIEW_URL)
        {
            meta.url = url;
            meta.favicon_url.clear();
        }
        if let Some(title) = ev.title
            && !meta.url.starts_with(TERMINAL_WEBVIEW_URL)
        {
            meta.title = title;
        }
        if let Some(favicon) = ev.favicon_url {
            meta.favicon_url = favicon;
        }
    }
}

impl Browser {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        url: &str,
    ) -> impl Bundle {
        (
            Self,
            vmux_core::PageMetadata {
                title: url.to_string(),
                url: url.to_string(),
                favicon_url: String::new(),
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                    ..default()
                },
                ..default()
            })),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        )
    }
}

pub fn layout_chrome_bundle(
    host_window: Entity,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> impl Bundle {
    (
        LayoutChrome,
        Browser,
        HostWindow(host_window),
        WebviewTransparent,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ZIndex(2),
        WebviewSource::new(LAYOUT_WEBVIEW_URL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
            base: StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                ..default()
            },
            ..default()
        })),
        WebviewSize(Vec2::new(1280.0, 720.0)),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Inherited,
        Pickable {
            should_block_lower: false,
            is_hoverable: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_test_chrome(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) {
        let host = commands.spawn_empty().id();
        commands.spawn(layout_chrome_bundle(host, &mut meshes, &mut webview_mt));
    }

    #[test]
    fn layout_chrome_does_not_block_pointer_events_below_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.add_systems(Startup, spawn_test_chrome);
        app.update();

        let pickable = app
            .world_mut()
            .query_filtered::<&Pickable, With<LayoutChrome>>()
            .single(app.world())
            .expect("layout chrome pickable");

        assert_eq!(
            pickable,
            &Pickable {
                should_block_lower: false,
                is_hoverable: true,
            }
        );
    }
}
