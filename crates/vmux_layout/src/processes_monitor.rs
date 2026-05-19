use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;

use crate::chrome::Browser;
use crate::event::SERVICES_WEBVIEW_URL;
use crate::window::WEBVIEW_MESH_DEPTH_BIAS;

#[derive(Component)]
pub struct ProcessesMonitor;

impl ProcessesMonitor {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SERVICES_WEBVIEW_URL),
                ResolvedWebviewUri(SERVICES_WEBVIEW_URL.to_string()),
                PageMetadata {
                    title: "Background Services".to_string(),
                    url: SERVICES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
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
            ),
        )
    }
}
