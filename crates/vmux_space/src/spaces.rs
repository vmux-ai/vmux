use std::path::{Path, PathBuf};

use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::profile;
use vmux_layout::cef::Browser;
use vmux_layout::window::WEBVIEW_MESH_DEPTH_BIAS;

use crate::event::{SPACES_WEBVIEW_URL, SpaceRow};
use crate::model::{
    DEFAULT_SPACE_ID, SpaceRecord, SpaceRegistry, default_space_record, registry_path,
    space_layout_path_for,
};

#[derive(Resource, Clone, Debug)]
pub struct ActiveSpace {
    pub record: SpaceRecord,
}

impl Default for ActiveSpace {
    fn default() -> Self {
        let registry = read_space_registry_from(&profile::shared_data_dir());
        let record = registry
            .spaces
            .iter()
            .find(|space| space.id == DEFAULT_SPACE_ID)
            .cloned()
            .or_else(|| registry.spaces.first().cloned())
            .unwrap_or_else(default_space_record);
        Self { record }
    }
}

impl ActiveSpace {
    pub fn layout_path(&self) -> PathBuf {
        space_layout_path_for(
            &profile::shared_data_dir(),
            &self.record.id,
            &self.record.profile,
        )
    }
}

pub fn read_space_registry_from(root: &Path) -> SpaceRegistry {
    let mut registry = std::fs::read_to_string(registry_path(root))
        .ok()
        .and_then(|body| ron::de::from_str::<SpaceRegistry>(&body).ok())
        .unwrap_or_default();
    if registry.spaces.is_empty() {
        registry.spaces.push(default_space_record());
    }
    if !registry
        .spaces
        .iter()
        .any(|space| space.id == DEFAULT_SPACE_ID)
    {
        registry.spaces.insert(0, default_space_record());
    }
    registry
}

pub fn active_space_rows(active: &ActiveSpace, active_stack_count: usize) -> Vec<SpaceRow> {
    let registry = read_space_registry_from(&profile::shared_data_dir());
    registry
        .spaces
        .into_iter()
        .map(|space| {
            let is_active = space.id == active.record.id;
            SpaceRow {
                id: space.id.clone(),
                name: space.name.clone(),
                profile: space.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect()
}

#[derive(Component)]
pub struct Spaces;

impl Spaces {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SPACES_WEBVIEW_URL),
                ResolvedWebviewUri(SPACES_WEBVIEW_URL.to_string()),
                PageMetadata {
                    title: "Spaces".to_string(),
                    url: SPACES_WEBVIEW_URL.to_string(),
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
